## Support Type Literal Returns
Following is broken:
```ts
type User = { 
	id: string;
}

export type Mutation {
	findUser = (input: { name: string }) => Promise<{ user: User, success: true}>
}
```
We have to create an output type, ex: `FindUserOutput`

## Fix Aliased Types
If we have the following type:
```ts
// types.ts
type Person = {
	name: string
}

// schema.ts
import { Person } from './types'

export type User = Person

export type Query = {
	findUser = (input: { name: string }) => Promise<User | null>
}
```

TS compiler API only expands one level, so `Query`'s expanded type becomes:
```ts
export type Query = {
	findUser = (input: { name: string }) => Promise<Person | null>
}
```

Solution is simple, when we visit type alias declarations we can check if the type if the node is a TypeReferenceNode and expand it there.

## Support exporting an import
We should be able to do the following:
```ts
import { Person } from './types'

export { Person }
```