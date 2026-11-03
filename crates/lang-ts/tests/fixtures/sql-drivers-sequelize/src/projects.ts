declare const sequelize: any;
declare const QueryTypes: any;

export const projects = () => sequelize.query('SELECT id FROM projects', { type: QueryTypes.SELECT });
