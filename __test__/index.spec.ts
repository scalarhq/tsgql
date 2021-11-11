import test from 'ava'

const { loadBinding } = require("@node-rs/helper");
const native = loadBinding(__dirname, "../core", "@modfy/tsgql");

test('works', (t) => {
  const types = `
    type User = {
      id: number,
      name: string
      age?: number
    }`
  const out = native.generateSchema(types, JSON.stringify({User: 0}), `{
    "syntax": "typescript",
    "tsx": true,
    "decorators": false,
    "dynamicImport": false
  }`)

  t.is(out, `type User {
  id: Int!
  name: String!
  age: Int
}
`)
})
