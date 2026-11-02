import { initReactI18next } from 'react-i18next';
import { i18n } from './i18n';

i18n.use(`${process.env.BASE}/plugins`, initReactI18next);
