# tsgql
Making Typescript and GraphQL a pleasant experience.

## What is this?
tsgql is an experimental GraphQL code generator that takes a different approach, instead of generating types from your GraphQL schema, you define your Typescript types first and use that to generate the schema.

This allows you to use the flexibility and expressiveness of Typescript's type system to create GraphQL schemas and reduce boilerplate, unlike alternatives such as [type-graphql](https://github.com/MichalLytek/type-graphql), which force you to use clunky and cumbersome classes/decorators.

It allows you to do things like this:
```typescript
export type User = {
  id: string;
  name: string;
  age: number;
};

export type Mutation = {
  updateUser: (args: { id: User['id'], fields: Partial<Omit<User, 'id'>> }) => Promise<User>
}
```
Which generates the following GraphQL schema:
```graphql
type User {
  id: String!
  name: String!
  age: Int!
}

input UpdateUserInputFields {
  name: String
  age: Int
}

type Mutation {
  updateUser(id: String!, fields: UpdateUserInputFields!): User!
}

```

Check out the examples folder for sample usage.

[Read more here](https://zackoverflow.dev/writing/tsgql).

## How it works
tsgql uses SWC to parse your Typescript schema into an AST, which is then walked and used to generate a GraphQL schema with [apollo-encoder](https://github.com/apollographql/apollo-rs/tree/main/crates/apollo-encoder).

Before the SWC step, we run your Typescript types through the TS Compiler API, which reduces them into a simpler form. This is necessary because SWC provides no such type system utilities.

This is an experimental internal tool we use at Modfy, and is subject to breaking changes and general instability.
