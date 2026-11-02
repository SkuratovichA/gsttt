import axios from 'axios';

declare const config: { url: string };
declare const id: string;

export const dynamic = () => axios.request(config);
export const templated = () => axios.request({ url: `/u/${id}` });
