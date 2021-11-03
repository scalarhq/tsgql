const { loadBinding } = require("@node-rs/helper");
const { reduceTypes, createReducer } = require("./dist");
const fs = require("fs");

/**
 * __dirname means load native addon from current dir
 * 'core' is the name of native addon
 * the second arguments was decided by `napi.name` field in `package.json`
 * the third arguments was decided by `name` field in `package.json`
 * `loadBinding` helper will load `core.[PLATFORM].node` from `__dirname` first
 * If failed to load addon, it will fallback to load from `@tsgql/core-[PLATFORM]`
 */
const native = loadBinding(__dirname, "core", "@tsgql/core");

const defaultArgs = {
  tsconfigPath: "./tsconfig.json",
  schema: "./src/schema.ts",
  out: "./schema.graphql",
};

const readConfig = (path) => {
  const conf = require(path)
  if (!conf.tsconfigPath) conf.tsconfigPath = defaultArgs.tsconfigPath
  if (!conf.schema) conf.tsconfigPath = defaultArgs.schema
  if (!conf.out) conf.tsconfigPath = defaultArgs.out
  return conf
}

const run = () => {
  let path = "./tql.config.js";
  if (process.argv.length > 2) {
    path = process.argv[2];
  }
  const { tsconfigPath, schema, out } = readConfig(path)

  const reducer = createReducer({tsconfigPath, path: schema})
  const [reduced, manifest] = reducer.generate();
  console.log('Reduced', reduced)

  native.generateSchema(
    reduced,
    JSON.stringify(manifest),
    `{
            "syntax": "typescript",
            "tsx": true,
            "decorators": false,
            "dynamicImport": false
      }`,
    out
  );
};

run();
