declare const sql: any;
declare const x: string;

export const lowered = () => sql`lower(${x})`;
