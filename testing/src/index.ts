import * as fs from 'fs';
import * as path from 'path';
import {
  Node,
  FunctionTypeNode,
  NamedNodeSpecific,
  NamedNodeSpecificBase,
  ParameterDeclaration,
  Project,
  PropertySignature,
  ts,
  Type,
  TypeAliasDeclaration,
  TypeAliasDeclarationStructure,
  TypedNode,
  TypeLiteralNode,
  ReturnTypedNode,
  TypeReferenceNode,
  ExportedDeclarations,
} from 'ts-morph';
import { TypeReducer } from './lib';

const prelude = `
type ___ExpandRecursively<T> = T extends object ? T extends infer O ? {
    [K in keyof O]: ___ExpandRecursively<O[K]>;
} : never : T;
type ___Expand<T> = T extends infer O ? { [K in keyof O]: O[K] } : never;
`;

const project = new Project();
const fileText = fs.readFileSync(path.join(__dirname, './src/input.ts'), {
  encoding: 'utf8',
});

const typeFile = project.createSourceFile('types.ts', 'export type Input<T extends Record<string, any>> = T;')
const sourceFile = project.createSourceFile('main.ts', prelude + fileText);

console.log(project.getPreEmitDiagnostics())

const reducer = new TypeReducer(project, sourceFile)

const output = reducer.generate()
console.log('\nFINISHED REDUCING TYPES:')
console.log(output)
fs.writeFileSync('./cleaned.ts', output);
