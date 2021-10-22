
/**
 * Marker type to indicate that the type passed in the generic
 * parameter should generate a GraphQL Input type.
 * 
 * Example:
 * ```
 * export type CreateUserInput = Input<{
 *   name: string
 * }>
 * ```
 * This will generate the following GraphQL Schema:
 * ```graphql
 * input CreateUserInput {
 *   name: String!
 * }
 * ```
 */
export type Input<T extends Record<string, any>> = T;