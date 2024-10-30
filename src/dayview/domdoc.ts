import { parseHTML } from "linkedom";

export function domdoc() {
  const { document } = parseHTML(`
<!DOCTYPE html>
<html>
  <head>
    <title>dayview</title>
    <meta http-equiv="refresh" content="5">
  </head>
  <body>
    <h1>dayview</h1>
    <div id="app"></div>
  </body>
</html>
`);
  return document;
}
