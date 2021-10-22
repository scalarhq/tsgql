const { reduceTypes } = require('../dist')

const prelude = `
type ___ExpandRecursively<T> = T extends object ? T extends infer O ? {
    [K in keyof O]: ___ExpandRecursively<O[K]>;
} : never : T;
type ___Expand<T> = T extends infer O ? { [K in keyof O]: O[K] } : never;
`;

const output = run({ tsconfigPath: './tsconfig.json', path: './src/input.ts' })
console.log('\nFINISHED REDUCING TYPES:')
console.log(output)