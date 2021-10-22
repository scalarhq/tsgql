import {
  Project,
} from 'ts-morph';
import { TypeReducer } from './lib';

export const reduceTypes = ({ tsconfigPath, path } = {tsconfigPath: './tsconfig.json', path: './src/schema.ts' }) => {
  const project = new Project({ tsConfigFilePath: tsconfigPath })
  project.compilerOptions.set({strict: false})
  const sourceFile = project.getSourceFile(path)
  if (!sourceFile) {
    throw new Error('Schema file not found')
  }

  const diagnostics = project.getPreEmitDiagnostics()
  if (diagnostics.length) {
    console.error(diagnostics)
    throw new Error('Aborting because of TSC errors')
  }

  const reducer = new TypeReducer(project, sourceFile)
  return reducer.generate()
}