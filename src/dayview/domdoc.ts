import { parseHTML } from "linkedom";

export function domdoc(title: string = "") {
  const { document } = parseHTML(`
<!DOCTYPE html>
<html>
  <head>
    <meta charset="UTF-8">
    <title>${title}</title>
    <style>
      body { margin: 0; padding: 0; background: #000; }
      svg { display: block; margin: 0 auto; }
    </style>
  </head>
  <body style="background-color: black">
  </body>
</html>
`);
  return document;
}
