import { ArgumentParser } from "argparse";
import { dbArgs } from "../input/dbArgs";
import { MainDatumArgs } from "../input/mainArgs";
import { nowview } from "../dayview/nowview";
import { parseIfNeeded } from "../utils/parseIfNeeded";
import { connectDb } from "../auth/connectDb";
import fs from "fs";
import xmlFormatter from "xml-formatter";

export const nowviewArgs = new ArgumentParser({
  add_help: false,
  parents: [],
});

nowviewArgs.add_argument("--width", {
  help: "width of the svg",
  type: "int",
});

nowviewArgs.add_argument("--height", {
  help: "height of the svg",
  type: "int",
});

nowviewArgs.add_argument("--output-file", "-o", {
  help: "output file, should have a .html or .svg extension",
  type: "str",
  dest: "outputFile",
});

nowviewArgs.add_argument("--watch", "-w", {
  help: "watch for changes and update every minute",
  action: "store_true",
  dest: "watch",
});

export const nowviewCmdArgs = new ArgumentParser({
  description: "View the current state and last 15 minutes",
  prog: "datum nowview",
  usage: `%(prog)s`,
  parents: [nowviewArgs, dbArgs],
});

export type NowviewCmdArgs = MainDatumArgs & {
  width?: number;
  height?: number;
  outputFile?: string;
  watch?: boolean;
};

export async function nowviewCmd(
  args: NowviewCmdArgs | string | string[],
  preparsed?: Partial<NowviewCmdArgs>,
): Promise<string> {
  args = parseIfNeeded(nowviewCmdArgs, args, preparsed);
  const db = connectDb(args);

  if (args.watch) {
    console.log("watching for changes");
    const emitter = db.changes({ since: "now", live: true });

    // Function to redraw the view
    const redraw = async () => {
      console.log("redrawing nowview");
      const svgContent = await nowview(args);

      if (args.outputFile) {
        if (args.outputFile.endsWith(".svg")) {
          const xmlDeclaration = '<?xml version="1.0" encoding="UTF-8" standalone="no"?>\n';
          const prettySvg = xmlFormatter(svgContent);
          fs.writeFileSync(args.outputFile, xmlDeclaration + prettySvg);
        } else if (args.outputFile.endsWith(".html")) {
          const htmlContent = `
<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8">
  <title>Nowview - Last 15 Minutes</title>
  <style>
    body { margin: 0; padding: 0; background: #000; }
    svg { display: block; margin: 0 auto; }
  </style>
</head>
<body>
  ${svgContent}
</body>
</html>`;
          const prettyHtml = xmlFormatter(htmlContent);
          fs.writeFileSync(args.outputFile, prettyHtml);
        } else {
          throw new Error("output file must have a .html or .svg extension");
        }
      }

      return svgContent;
    };

    // Initial draw
    await redraw();

    // Set up redraw on changes or every minute
    while (true) {
      await Promise.all([
        // Wait for either a database change or 1 minute to pass
        new Promise((resolve) => {
          const timeout = setTimeout(resolve, 60 * 1000); // 1 minute
          emitter.once("change", () => {
            clearTimeout(timeout);
            resolve(undefined);
          });
        }),
        redraw(),
      ]);
    }
  }

  // Single run mode
  const svgContent = await nowview(args);

  if (args.outputFile === undefined) {
    return svgContent;
  }

  if (args.outputFile.endsWith(".svg")) {
    // Add XML declaration, SVG namespace, and ensure all required namespaces are included
    const xmlDeclaration = '<?xml version="1.0" encoding="UTF-8" standalone="no"?>\n';
    const prettySvg = xmlFormatter(svgContent);
    fs.writeFileSync(args.outputFile, xmlDeclaration + prettySvg);
    return prettySvg;
  } else if (args.outputFile.endsWith(".html")) {
    const htmlContent = `
<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8">
  <title>Nowview - Last 15 Minutes</title>
  <style>
    body { margin: 0; padding: 0; background: #000; }
    svg { display: block; margin: 0 auto; }
  </style>
</head>
<body>
  ${svgContent}
</body>
</html>`;
    const prettyHtml = xmlFormatter(htmlContent);
    fs.writeFileSync(args.outputFile, prettyHtml);
    return svgContent;
  } else {
    throw new Error("output file must have a .html or .svg extension");
  }
}