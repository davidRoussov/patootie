import { go } from './lib';
import 'source-map-support/register';

(async () => {
  const args = process.argv.slice(2);

  if (args[0] == 'go') {
    await go();
  }
})();
