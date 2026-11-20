import solid from "eslint-plugin-solid";
import tseslint from "typescript-eslint";
export default [{
  files: ["src/**/*.tsx"],
  languageOptions: { parser: tseslint.parser },
  plugins: { solid },
  rules: { ...solid.configs.typescript.rules },
}];
