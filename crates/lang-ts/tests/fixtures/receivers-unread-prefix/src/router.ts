import { makeRouter } from 'mini-router';

export const dynamic = makeRouter({ prefix: process.env.PREFIX });
export const fixed = makeRouter({ prefix: '/v1' });
export const bare = makeRouter({});

function list() {}

dynamic.get('/own', list);
fixed.get('/own', list);
bare.get('/own', list);
