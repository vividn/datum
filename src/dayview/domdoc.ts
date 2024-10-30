import { parseHTML } from "linkedom";

export function domdoc() {
  const { document } = parseHTML(`
<!DOCTYPE html>
<html>
  <head>
    <title>dayview</title>
  </head>
  <body>
    <h1>dayview</h1>
    <div id="app"></div>
  </body>
`);
  return document;
}
