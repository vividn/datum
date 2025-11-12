import { fileURLToPath } from "url";
import { dirname } from "path";
import { MyError } from "../errors.js";
import {
  DatumView,
  StringifiedDatumView,
  ViewPayload,
  ViewPayloadViews,
  ConflictingReduceError,
} from "./DatumView.js";
import { transformSync, TransformOptions } from "@babel/core";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const couchdbBabelTransformOptions: TransformOptions = {
  filename: "view.ts",
  presets: [
    "@babel/preset-typescript",
    [
      "@babel/preset-env",
      {
        targets: {
          esmodules: false, // Disable ES modules to target older JavaScript
          browsers: ["firefox 52"], // Couchdb doesn't support many modern JS features. Even though it claims to be SpiderMonkey 78, it doesn't support the ...rest op for exampe.
        },
        bugfixes: true,
      },
    ],
  ],
  retainLines: true,
  sourceType: "script",
  sourceMaps: false,
  babelrc: false,
  cwd: __dirname,
  configFile: false,
};

export class BabelTransformError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, BabelTransformError.prototype);
  }
}

function toCouchDbJs(fnStr: string): string {
  // For special reduce operations, e.g. _count just return the string directly
  if (fnStr.startsWith("_")) {
    return fnStr;
  }
  const transformed = transformSync(fnStr, couchdbBabelTransformOptions);
  if (!transformed || !transformed.code) {
    throw new BabelTransformError(
      "Failed to transform view function into couchdb compatible code",
    );
  }
  // babel returns the function with a trailing semicolon, which is not allowed in couchdb views, so remove it
  let code = transformed.code.slice(0, -1);

  // Remove __name helper calls that Babel may inject for debugging
  code = code.replace(/__name\([^,]+,\s*"[^"]+"\);?/g, '');

  return code;
}

export function datumViewToViewPayload(
  datumView: DatumView<any, any, any, any, any> | StringifiedDatumView,
): ViewPayload {
  const views: ViewPayloadViews = {};
  const name = datumView.name;
  const mapStr = toCouchDbJs(datumView.map.toString());
  const options = datumView.options;
  const defaultReduce = datumView.reduce;
  const namedReduce = datumView.namedReduce;

  if (datumView.reduce && datumView.namedReduce?.[name]) {
    throw new ConflictingReduceError(name);
  }

  if (defaultReduce) {
    const reduceStr = toCouchDbJs(defaultReduce.toString());
    views[name] = {
      map: mapStr,
      reduce: reduceStr,
      options,
    };
  }

  if (namedReduce) {
    for (const reduceName in datumView.namedReduce) {
      const reduceStr = toCouchDbJs(namedReduce[reduceName].toString());
      views[reduceName] = {
        map: mapStr,
        reduce: reduceStr,
        options,
      };
    }
  }

  // set default view with just map function if no default reduce exists
  if (!views[name]) {
    views[name] = {
      map: mapStr,
      options,
    };
  }

  return {
    _id: `_design/${datumView.name}`,
    views: views,
    meta: {},
  };
}
