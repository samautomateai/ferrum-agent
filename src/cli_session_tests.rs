use super::*;

#[cfg(test)]
mod cli_session_contracts {
    use super::*;
    use clap::Parser;

    fn args(root: &Path) -> Args {
        let mut args = Args::parse_from(["ferrum", "--api-key", "test"]);
        args.workspace = root.to_path_buf();
        args
    }

    #[test]
    fn cli_requires_database_for_import_and_export() {
        let dir = tempfile::tempdir().unwrap();
        for is_import in [true, false] {
            let mut config = args(dir.path());
            if is_import {
                config.import_json = Some(dir.path().join("input.json"));
            } else {
                config.export_json = Some(dir.path().join("output.json"));
            }
            assert!(main_with_args(config).unwrap_err().to_string().contains("--session-db"));
        }
    }

    #[test]
    fn cli_import_and_export_roundtrip_without_llm() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.json");
        let output = dir.path().join("output.json");
        let db = dir.path().join("nested").join("sessions.sqlite");
        let expected = vec![
            json!({"role":"user","content":"hello"}),
            json!({"role":"assistant","content":"world"}),
        ];
        fs::write(&input, serde_json::to_vec(&expected).unwrap()).unwrap();
        let mut config = args(dir.path());
        config.session_db = Some(db.clone());
        config.session_id = "cli-test".into();
        config.import_json = Some(input);
        config.export_json = Some(output.clone());
        main_with_args(config).unwrap();
        let actual: Vec<Value> = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(load_sqlite(&open_store(&db).unwrap(), "cli-test").unwrap(), expected);
    }

    #[test]
    fn cli_import_rejects_invalid_json_without_corrupting_session() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("broken.json");
        let db = dir.path().join("sessions.sqlite");
        fs::write(&input, "{bad").unwrap();
        let mut config = args(dir.path());
        config.session_db = Some(db.clone());
        config.import_json = Some(input);
        assert!(main_with_args(config).is_err());
        assert!(load_sqlite(&open_store(&db).unwrap(), "default").unwrap().is_empty());
    }

    #[test]
    fn cli_export_of_unknown_session_is_empty_array() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("export.json");
        let mut config = args(dir.path());
        config.session_db = Some(dir.path().join("sessions.sqlite"));
        config.export_json = Some(output.clone());
        main_with_args(config).unwrap();
        let actual: Vec<Value> = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
        assert!(actual.is_empty());
    }

    #[test]
    fn cli_import_can_be_followed_by_export_in_separate_invocations() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("sessions.sqlite");
        let input = dir.path().join("import.json");
        let output = dir.path().join("export.json");
        fs::write(&input, r#"[{"role":"user","content":"persisted"}]"#).unwrap();
        let mut import = args(dir.path());
        import.session_db = Some(db.clone());
        import.import_json = Some(input);
        main_with_args(import).unwrap();
        let mut export = args(dir.path());
        export.session_db = Some(db);
        export.export_json = Some(output.clone());
        main_with_args(export).unwrap();
        let history: Vec<Value> = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
        assert_eq!(history[0]["content"], "persisted");
    }
}
