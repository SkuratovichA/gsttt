import { Settings } from './settings';
import { FakeSettings } from './fake';

export const providers = [{ provide: Settings, useClass: FakeSettings }];

export const port = (settings: Settings) => settings.read('PORT');
