export default {
  presets: [
    ["@babel/preset-env", { targets: { node: "current" } }],
    "@babel/preset-typescript",
  ],
  "retainLines": true,
  "sourceMap": "inline"
};
