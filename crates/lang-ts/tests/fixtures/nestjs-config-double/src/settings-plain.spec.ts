import { Settings } from './settings';

export const port = (settings: Settings) => settings.read('PORT');
