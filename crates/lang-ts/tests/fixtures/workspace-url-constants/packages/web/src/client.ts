import axios from 'axios';
import { API_PREFIX, USERS } from '@acme/shared';

export const byTemplate = () => axios.get(`${API_PREFIX}/users/list`);
export const byConcatenation = () => axios.get(API_PREFIX + USERS);
