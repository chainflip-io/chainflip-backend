#!/usr/bin/env node --trace-uncaught

import { Project, TypeFormatFlags } from 'ts-morph';
import * as path from 'path';
import url from 'url';

const __dirname = path.dirname(url.fileURLToPath(import.meta.url));
const rootPath = path.resolve(__dirname, '../');

const printTypes = async (tsConfigPath, filePath, names) => {
  const project = new Project({
    tsConfigFilePath: tsConfigPath,
  });
  const sourceFile = project.getSourceFileOrThrow(filePath);

  for (const name of names) {
    const statements = sourceFile
      .getStatements()
      .filter((s) => s.compilerNode.name?.escapedText === name);

    for (const statement of statements) {
      const statementType = statement.getType();

      console.log('type ' + name + ' =');
      if (statementType.isUnion()) {
        for (const unionType of statementType.getUnionTypes()) {
          console.log(
            '| ' + unionType.getText(undefined, TypeFormatFlags.NoTruncation),
          );
        }
      } else {
        console.log(
          statementType.getText(undefined, TypeFormatFlags.NoTruncation),
        );
      }
      console.log();
    }
  }
};

void printTypes(
  path.resolve(rootPath, 'packages/shared/tsconfig.json'),
  path.resolve(rootPath, 'packages/shared/src/vault/schemas.ts'),
  ['ExecuteSwapParams', 'ExecuteCallParams', 'ExecuteOptions'],
);

void printTypes(
  path.resolve(rootPath, 'packages/sdk/tsconfig.json'),
  path.resolve(rootPath, 'packages/sdk/src/swap/types.ts'),
  ['SwapStatusResponse'],
);
