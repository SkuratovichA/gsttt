import axios from 'axios';

export class Runner {
  constructor(private readonly baseUrl: string) {}

  report(body: unknown) {
    return axios.post(new URL('build-runner/success', this.baseUrl).href, body);
  }
}

export const users = () => fetch(new URL('/users', process.env.API_URL));
