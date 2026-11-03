import Fallback from './head';

export function halt(fallback: Fallback) {
  fallback.stop();
}
