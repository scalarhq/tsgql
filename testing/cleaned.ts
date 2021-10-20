type User = { id: string; name: string; karma: number; }
type Loot = { rarity: number; name: string; }
type Player = { user: User; level: number; }
type CreatePlayerInput = { user?: User; level?: number; }
type CreatePlayerAndUserInput = { level?: number; id?: string; name?: string; karma?: number; }
type Query = { findUser: (input: {    id?: string;    name?: string;}) => Promise<User | null>; }
type Mutation = { createUser: (input: {    name?: string;    karma?: number;}) => Promise<User | null>; createLoot: (input: {    name?: string;}) => Promise<CreateLootOutput>; }
type CreateLootOutput = { success: boolean; loot?: Loot; }