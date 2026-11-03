declare const getDb: () => any;

export const audits = () => getDb().query('SELECT id FROM audits');

export const nothing = () => /SELECT/.exec('SELECT id FROM nothing');
