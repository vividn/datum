import { parseHTML } from "linkedom";

export function domdoc() {
  const { document } = parseHTML(`
<!DOCTYPE html>
<html>
  <head>
    <title>dayview</title>
    <script type="text/javascript" src="https://livejs.com/live.js"></script>
  </head>
  <body style="background-color: black">
  </body>
</html>
`);
  return document;
}
