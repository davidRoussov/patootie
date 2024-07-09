import puppeteer from 'puppeteer';


export const go = async (url: string): Promise<void> => {

  const browser = await puppeteer.launch({
    headless: true,
  });

  const page = await browser.newPage();

  await page.goto(url);



  //const screenshot = await page.screenshot({ encoding: 'base64' });


  const html = await page.content();
  console.log('html', html);





  await browser.close();
};

