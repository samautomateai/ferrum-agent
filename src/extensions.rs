//! Trusted Boa extension host. M1.3 supports settled Promise handlers.
//! This module is not a sandbox: never load untrusted extension source.
use anyhow::{bail, Context, Result};
use boa_engine::{Context as JsContext, Source};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_SOURCE_BYTES: usize = 256 * 1024;
const MAX_ARGUMENT_BYTES: usize = 1024 * 1024;
const MAX_RESULT_BYTES: usize = 1024 * 1024;
const MAX_PROMISE_PUMPS: usize = 32;
const BOOTSTRAP: &str = r#"
  globalThis.__piRsRegistry = { tools: [], definitions: [], commands: [], events: [] };
  globalThis.__piRsHandlers = Object.create(null);
  globalThis.__piRsEventHandlers = Object.create(null);
  globalThis.__piRsInvocation = null;
  globalThis.__piRsCall = (name, args) => {
    const state = { status: 'pending', value: null };
    globalThis.__piRsInvocation = state;
    try {
      Promise.resolve(__piRsHandlers[name](args)).then(
        value => {
          try {
            const json = JSON.stringify(value);
            if (json === undefined) throw new TypeError('non-JSON result');
            state.value = json;
            state.status = 'fulfilled';
          } catch (error) {
            state.value = String(error);
            state.status = 'rejected';
          }
        },
        error => { state.value = String(error); state.status = 'rejected'; }
      );
    } catch (error) {
      state.value = String(error);
      state.status = 'rejected';
    }
  };
  globalThis.__piRsEmit = (event, payload) => {
    const handlers = __piRsEventHandlers[event] || [];
    const state = { status: 'pending', value: null };
    globalThis.__piRsInvocation = state;
    Promise.all(handlers.map(fn => Promise.resolve().then(() => fn(payload)))).then(
      () => { state.status = 'fulfilled'; state.value = 'null'; },
      error => { state.status = 'rejected'; state.value = String(error); }
    );
  };
  globalThis.pi = Object.freeze({
    registerTool(tool) {
      if (!tool || typeof tool.name !== 'string' || !tool.name.trim()) {
        throw new TypeError('tool name is required');
      }
      if (typeof tool.execute !== 'function') {
        throw new TypeError('tool execute function is required');
      }
      if (Object.hasOwn(__piRsHandlers, tool.name)) {
        throw new TypeError('duplicate tool name');
      }
      if (tool.parameters !== undefined && (!tool.parameters ||
          typeof tool.parameters !== 'object' || Array.isArray(tool.parameters))) {
        throw new TypeError('parameters must be a JSON schema object');
      }
      const definition = {type:'function',function:{name:tool.name,
        description:typeof tool.description === 'string' ? tool.description : '',
        parameters:tool.parameters || {type:'object',properties:{}}}};
      JSON.stringify(definition);
      __piRsRegistry.definitions.push(definition);
      __piRsHandlers[tool.name] = tool.execute;
      __piRsRegistry.tools.push(tool.name);
    },
    registerCommand(name) {
      if (typeof name !== 'string' || !name.trim()) {
        throw new TypeError('command name is required');
      }
      __piRsRegistry.commands.push(name);
    },
    on(event, handler) {
      if (typeof event !== 'string' || !event.trim()) {
        throw new TypeError('event name is required');
      }
      if (typeof handler !== 'function') throw new TypeError('event handler required');
      (__piRsEventHandlers[event] ||= []).push(handler);
      if (!__piRsRegistry.events.includes(event)) __piRsRegistry.events.push(event);
    }
  });
"#;

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionManifest {
    pub tools: Vec<String>,
    #[serde(default)]
    pub definitions: Vec<Value>,
    pub commands: Vec<String>,
    pub events: Vec<String>,
}

/// One extension per context. Boa contexts are not Send; keep on one thread.
pub struct TrustedExtensionHost {
    context: JsContext,
    manifest: ExtensionManifest,
}

impl TrustedExtensionHost {
    pub fn load(source: &str) -> Result<Self> {
        if source.len() > MAX_SOURCE_BYTES {
            bail!("extension source exceeds 256 KiB limit");
        }
        let mut context = JsContext::default();
        context
            .eval(Source::from_bytes(BOOTSTRAP))
            .map_err(|e| anyhow::anyhow!("bootstrap failed: {e}"))?;
        context
            .eval(Source::from_bytes(source))
            .map_err(|e| anyhow::anyhow!("extension failed: {e}"))?;
        let value = context
            .eval(Source::from_bytes(
                "JSON.stringify(globalThis.__piRsRegistry)",
            ))
            .map_err(|e| anyhow::anyhow!("registry failed: {e}"))?;
        let json = value
            .to_string(&mut context)
            .map_err(|e| anyhow::anyhow!("registry conversion failed: {e}"))?
            .to_std_string_escaped();
        let manifest = serde_json::from_str(&json).context("invalid extension registry")?;
        Ok(Self { context, manifest })
    }

    pub fn manifest(&self) -> &ExtensionManifest {
        &self.manifest
    }

    #[allow(dead_code)] // Exposed for future tool schema registration.
    pub fn definitions(&self) -> &[Value] {
        &self.manifest.definitions
    }

    /// Invoke a handler, including Promises resolved by Boa's microtask queue.
    /// This does not provide timers, external I/O, preemption, or cancellation.
    pub fn invoke(&mut self, name: &str, arguments: &Value) -> Result<Value> {
        if !self
            .manifest
            .tools
            .iter()
            .any(|registered| registered == name)
        {
            bail!("unregistered extension tool: {name}");
        }
        let arguments_json = serde_json::to_string(arguments)?;
        if arguments_json.len() > MAX_ARGUMENT_BYTES {
            bail!("extension arguments exceed 1 MiB");
        }
        let name_literal = serde_json::to_string(name)?;
        let arguments_literal = serde_json::to_string(&arguments_json)?;
        let expression = format!("__piRsCall({name_literal}, JSON.parse({arguments_literal}))");
        self.context
            .eval(Source::from_bytes(&expression))
            .map_err(|e| anyhow::anyhow!("extension tool {name} failed: {e}"))?;
        self.settle(name)
    }

    pub fn emit(&mut self, event: &str, payload: &Value) -> Result<()> {
        if !self
            .manifest
            .events
            .iter()
            .any(|registered| registered == event)
        {
            return Ok(());
        }
        let event_literal = serde_json::to_string(event)?;
        let payload_literal = serde_json::to_string(payload)?;
        if payload_literal.len() > MAX_ARGUMENT_BYTES {
            bail!("event payload too large");
        }
        let expression = format!("__piRsEmit({event_literal}, {payload_literal})");
        self.context
            .eval(Source::from_bytes(&expression))
            .map_err(|e| anyhow::anyhow!("event dispatch failed: {e}"))?;
        self.settle(event).map(|_| ())
    }

    fn settle(&mut self, name: &str) -> Result<Value> {
        for _ in 0..MAX_PROMISE_PUMPS {
            self.context.run_jobs();
            let status = self
                .context
                .eval(Source::from_bytes("globalThis.__piRsInvocation.status"))
                .map_err(|e| anyhow::anyhow!("promise status failed: {e}"))?;
            let status = status
                .to_string(&mut self.context)
                .map_err(|e| anyhow::anyhow!("status conversion failed: {e}"))?
                .to_std_string_escaped();
            if status == "pending" {
                continue;
            }
            let value = self
                .context
                .eval(Source::from_bytes("globalThis.__piRsInvocation.value"))
                .map_err(|e| anyhow::anyhow!("promise result failed: {e}"))?;
            let result = value
                .to_string(&mut self.context)
                .map_err(|e| anyhow::anyhow!("result conversion failed: {e}"))?
                .to_std_string_escaped();
            if result.len() > MAX_RESULT_BYTES {
                bail!("extension result exceeds 1 MiB");
            }
            if status == "rejected" {
                bail!("extension tool {name} rejected: {result}");
            }
            return serde_json::from_str(&result).context("extension returned invalid JSON");
        }
        bail!("extension tool {name} promise did not settle; external async I/O is unsupported")
    }
}

#[cfg(test)]
pub fn inspect_trusted_extension(source: &str) -> Result<ExtensionManifest> {
    Ok(TrustedExtensionHost::load(source)?.manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SAMPLE: &str = r#"
        pi.registerTool({name:'review', execute: (args) => ({ok:true, path:args.path})});
        pi.registerCommand('hello', {});
        pi.on('session_start', () => {});
    "#;

    #[test]
    fn registers_tools_commands_and_events() {
        let manifest = inspect_trusted_extension(SAMPLE).unwrap();
        assert_eq!(manifest.tools, ["review"]);
        assert_eq!(manifest.commands, ["hello"]);
        assert_eq!(manifest.events, ["session_start"]);
    }

    #[test]
    fn event_handlers_preserve_state() {
        let source = r#"
            let count = 0;
            pi.on('session_start', async () => { count += 2; });
            pi.registerTool({name:'counter', execute:() => count});
        "#;
        let mut host = TrustedExtensionHost::load(source).unwrap();
        host.emit("session_start", &json!({})).unwrap();
        assert_eq!(host.invoke("counter", &json!({})).unwrap(), json!(2));
        host.emit("unknown", &json!({})).unwrap();
    }

    #[test]
    fn rejects_invalid_event_callback() {
        assert!(TrustedExtensionHost::load("pi.on('ready', 1);").is_err());
    }

    #[test]
    fn event_rejection_propagates() {
        let mut host =
            TrustedExtensionHost::load("pi.on('ready', () => { throw Error('bad event'); });")
                .unwrap();
        assert!(host.emit("ready", &json!({})).is_err());
    }

    #[test]
    fn invokes_registered_handler() {
        let mut host = TrustedExtensionHost::load(SAMPLE).unwrap();
        assert_eq!(
            host.invoke("review", &json!({"path":"src/main.rs"}))
                .unwrap(),
            json!({"ok":true,"path":"src/main.rs"})
        );
    }

    #[test]
    fn escapes_argument_strings() {
        let mut host = TrustedExtensionHost::load(SAMPLE).unwrap();
        let input = "\"); throw Error('injected'); // \n💻";
        assert_eq!(
            host.invoke("review", &json!({"path":input})).unwrap()["path"],
            input
        );
    }

    #[test]
    fn rejects_unknown_tool() {
        let mut host = TrustedExtensionHost::load(SAMPLE).unwrap();
        assert!(host.invoke("missing", &json!({})).is_err());
    }

    #[test]
    fn rejects_duplicate_registration() {
        assert!(TrustedExtensionHost::load(
            "pi.registerTool({name:'x',execute:()=>1});pi.registerTool({name:'x',execute:()=>2});"
        )
        .is_err());
    }

    #[test]
    fn rejects_missing_execute() {
        assert!(inspect_trusted_extension("pi.registerTool({name:'x'});").is_err());
    }

    #[test]
    fn rejects_missing_tool_name() {
        assert!(inspect_trusted_extension("pi.registerTool({execute:()=>1});").is_err());
    }

    #[test]
    fn rejects_invalid_javascript() {
        assert!(inspect_trusted_extension("const = ;").is_err());
    }

    #[test]
    fn rejects_oversized_extension() {
        assert!(inspect_trusted_extension(&" ".repeat(MAX_SOURCE_BYTES + 1)).is_err());
    }

    #[test]
    fn rejects_oversized_arguments() {
        let mut host = TrustedExtensionHost::load(SAMPLE).unwrap();
        assert!(host
            .invoke("review", &json!({"path":"x".repeat(MAX_ARGUMENT_BYTES)}))
            .is_err());
    }

    #[test]
    fn rejects_undefined_result() {
        let mut host =
            TrustedExtensionHost::load("pi.registerTool({name:'void',execute:()=>undefined});")
                .unwrap();
        assert!(host.invoke("void", &json!({})).is_err());
    }

    #[test]
    fn rejects_thrown_errors() {
        let mut host = TrustedExtensionHost::load(
            "pi.registerTool({name:'bad',execute:()=>{throw Error('failed')}});",
        )
        .unwrap();
        assert!(host.invoke("bad", &json!({})).is_err());
    }

    #[test]
    fn advertises_schema_and_invokes() {
        let mut host = TrustedExtensionHost::load(
            r#"pi.registerTool({
          name:'review',description:'Review a path',
          parameters:{type:'object',properties:{path:{type:'string'}},required:['path']},
          execute:(args)=>({reviewed:args.path})
        });"#,
        )
        .unwrap();
        assert_eq!(host.definitions()[0]["function"]["name"], "review");
        assert_eq!(
            host.definitions()[0]["function"]["parameters"]["required"][0],
            "path"
        );
        assert_eq!(
            host.invoke("review", &json!({"path":"src/lib.rs"}))
                .unwrap(),
            json!({"reviewed":"src/lib.rs"})
        );
    }

    #[test]
    fn rejects_bad_schema() {
        assert!(TrustedExtensionHost::load(
            "pi.registerTool({name:'bad',parameters:42,execute:()=>1});"
        )
        .is_err());
    }

    #[test]
    fn resolves_async_handler() {
        let mut host = TrustedExtensionHost::load(
            "pi.registerTool({name:'async',execute:async (args)=>({value:args.x+1})});",
        )
        .unwrap();
        assert_eq!(
            host.invoke("async", &json!({"x": 4})).unwrap(),
            json!({"value":5})
        );
    }

    #[test]
    fn reports_rejected_promise() {
        let mut host = TrustedExtensionHost::load(
            "pi.registerTool({name:'reject',execute:async()=>{throw Error('nope')}});",
        )
        .unwrap();
        assert!(host.invoke("reject", &json!({})).is_err());
    }

    #[test]
    fn rejects_never_settled_promise() {
        let mut host = TrustedExtensionHost::load(
            "pi.registerTool({name:'wait',execute:()=>new Promise(()=>{})});",
        )
        .unwrap();
        assert!(host.invoke("wait", &json!({})).is_err());
    }

    #[test]
    fn resolves_microtask_chain() {
        let mut host = TrustedExtensionHost::load(
            "pi.registerTool({name:'chain',execute:()=>Promise.resolve(2).then(x=>x*3)});",
        )
        .unwrap();
        assert_eq!(host.invoke("chain", &json!({})).unwrap(), json!(6));
    }

    #[test]
    fn separate_contexts_do_not_share_registrations() {
        let a = inspect_trusted_extension("pi.registerTool({name:'a',execute:()=>1});").unwrap();
        let b = inspect_trusted_extension("pi.registerTool({name:'b',execute:()=>2});").unwrap();
        assert_eq!(a.tools, ["a"]);
        assert_eq!(b.tools, ["b"]);
    }
}
