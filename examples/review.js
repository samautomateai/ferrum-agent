pi.registerTool({
  name: 'review_path',
  description: 'Echo a path for review (example extension)',
  parameters: {
    type: 'object',
    properties: {path: {type: 'string'}},
    required: ['path']
  },
  execute: (args) => ({reviewed: args.path})
});
