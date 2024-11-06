import { parseHTML } from "linkedom";

export function domdoc() {
  const { document } = parseHTML(`
<!DOCTYPE html>
<html>
  <head>
    <title>dayview</title>
    <atem http-equiv="refresh" content="5">
  </head>
  <body>
  </body>
</html>
`);
  return document;
}
