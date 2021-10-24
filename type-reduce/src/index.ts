import {
  Project,
} from 'ts-morph';
import { TypeReducer } from './lib';

const prelude = `
type ___ExpandRecursively<T> = T extends object ? T extends infer O ? {
    [K in keyof O]: ___ExpandRecursively<O[K]>;
} : never : T;
type ___Expand<T> = T extends infer O ? { [K in keyof O]: O[K] } : never;
`;

export const reduceTypes = ({ tsconfigPath, path } = {tsconfigPath: './tsconfig.json', path: './src/schema.ts' }) => {
  const project = new Project({ tsConfigFilePath: tsconfigPath })

  // With strict on Typescript turns optional types into unions:
  // optionalType?: string -> optionalType?: string | undefined
  project.compilerOptions.set({strict: false})

  const sourceFile = project.getSourceFile(path)
  if (!sourceFile) {
    throw new Error('Schema file not found')
  }
  
  sourceFile.insertText(0, prelude)

  const diagnostics = project.getPreEmitDiagnostics()
  if (diagnostics.length) {
    console.error(diagnostics)
    throw new Error('Aborting because of TSC errors')
  }

  const reducer = new TypeReducer(project, sourceFile)
  return reducer.generate()
}

export * from './types'