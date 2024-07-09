import { spawn } from 'child_process';
import puppeteer from 'puppeteer';

const parverse = async (html: string): Promise<any> => {
  return new Promise((resolve, reject) => {
    const parversion = '/Users/david/projects/parversion';

    let output = '';

    const cargoProcess = spawn('cargo', ['run'], { cwd: parversion });

    cargoProcess.stdin.write(html + '\n');
    cargoProcess.stdin.end();

    cargoProcess.stdout.on('data', (data) => {
      output += data.toString();
    });

    cargoProcess.stderr.on('data', (error) => {
      console.error('error', error.toString());
    });

    cargoProcess.on('close', (code) => {
      console.log(`Parversion exited with code: ${code}`);
      resolve(output);
    });
  });
};

export const go = async (url: string): Promise<void> => {

  const browser = await puppeteer.launch({
    headless: true,
  });

  const page = await browser.newPage();

  await page.goto(url);



  //const screenshot = await page.screenshot({ encoding: 'base64' });


  const html = await page.content();
  console.log('html', html);




  const output = await parverse(html);


  console.log(output);




  await browser.close();
};

