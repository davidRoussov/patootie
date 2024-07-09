import { go } from './lib';
import 'source-map-support/register';

(async () => {
  const args = process.argv.slice(2);
  const url = args[0];

  await go(url);
})();
