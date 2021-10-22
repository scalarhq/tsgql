type User = { id: string; name: string; karma: number; }
type Loot = { rarity: number; name: string; }
type Player = { user: User; level: number; }
type GetUserInput = { id?: string | undefined; name?: string | undefined; karma?: number | undefined; }
type Query = { getUser: (input: {    user?: GetUserInput;    karma?: number | undefined;}) => Promise<User[] | null | null>; }
type CreateLootOutput = { success: boolean; loot?: Loot | undefined; }