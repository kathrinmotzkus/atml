import * as assert from 'node:assert';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { loadWASM, OnigScanner, OnigString } from 'vscode-oniguruma';
import { INITIAL, parseRawGrammar, Registry } from 'vscode-textmate';

const extensionRoot = path.resolve(__dirname, '..', '..');
let grammarPromise: ReturnType<typeof createGrammar> | undefined;

suite('ATML TextMate grammar', () => {
  test('keeps English and German manifest localization complete', () => {
    const manifest = JSON.parse(
      fs.readFileSync(path.join(extensionRoot, 'package.json'), 'utf8'),
    ) as unknown;
    const english = loadMessages('package.nls.json');
    const german = loadMessages('package.nls.de.json');
    const placeholders = new Set<string>();

    collectPlaceholders(manifest, placeholders);
    assert.deepStrictEqual(new Set(Object.keys(english)), placeholders);
    assert.deepStrictEqual(new Set(Object.keys(german)), placeholders);
    for (const key of placeholders) {
      assert.ok(english[key].trim(), `empty English localization for ${key}`);
      assert.ok(german[key].trim(), `empty German localization for ${key}`);
    }
  });

  test('highlights all ATML Stage 1 constructs', async () => {
    const grammar = await loadGrammar();

    const fixture = fs.readFileSync(
      path.join(extensionRoot, 'test', 'fixtures', 'highlighting.atml'),
      'utf8',
    );
    const scopes = new Set<string>();
    let state = INITIAL;
    for (const line of fixture.split('\n')) {
      const result = grammar.tokenizeLine(line, state);
      state = result.ruleStack;
      for (const token of result.tokens) {
        for (const scope of token.scopes) {
          scopes.add(scope);
        }
      }
    }

    for (const expected of [
      'comment.line.number-sign.atml',
      'entity.name.type.enum.atml',
      'storage.type.enum.atml',
      'meta.table.atml',
      'meta.table.inherited.atml',
      'keyword.operator.inheritance.atml',
      'constant.other.enum-member.atml',
      'variable.other.reference.atml',
      'support.type.unit.atml',
      'keyword.operator.unit.atml',
    ]) {
      assert.ok(scopes.has(expected), `missing scope ${expected}`);
    }
  });

  test('tokenizes every official ATML example', async () => {
    const grammar = await loadGrammar();
    const examples = path.resolve(extensionRoot, '..', '..', '..', 'examples');
    const files = fs.readdirSync(examples).filter((file) => file.endsWith('.atml'));
    assert.ok(files.length > 0, 'no official ATML examples found');

    for (const file of files) {
      let state = INITIAL;
      const source = fs.readFileSync(path.join(examples, file), 'utf8');
      for (const line of source.split('\n')) {
        state = grammar.tokenizeLine(line, state).ruleStack;
      }
    }
  });
});

function loadMessages(file: string): Record<string, string> {
  return JSON.parse(fs.readFileSync(path.join(extensionRoot, file), 'utf8')) as Record<string, string>;
}

function collectPlaceholders(value: unknown, result: Set<string>): void {
  if (typeof value === 'string') {
    const match = /^%([^%]+)%$/.exec(value);
    if (match) {
      result.add(match[1]);
    }
    return;
  }
  if (Array.isArray(value)) {
    value.forEach((entry) => collectPlaceholders(entry, result));
    return;
  }
  if (value && typeof value === 'object') {
    Object.values(value).forEach((entry) => collectPlaceholders(entry, result));
  }
}

function loadGrammar(): ReturnType<typeof createGrammar> {
  grammarPromise ??= createGrammar();
  return grammarPromise;
}

async function createGrammar() {
  const wasm = fs.readFileSync(require.resolve('vscode-oniguruma/release/onig.wasm'));
  await loadWASM(wasm.buffer.slice(wasm.byteOffset, wasm.byteOffset + wasm.byteLength));
  const grammarPath = path.join(extensionRoot, 'syntaxes', 'atml.tmLanguage.json');
  const registry = new Registry({
    onigLib: Promise.resolve({
      createOnigScanner: (patterns) => new OnigScanner(patterns),
      createOnigString: (text) => new OnigString(text),
    }),
    loadGrammar: async (scopeName) => {
      if (scopeName !== 'source.atml') {
        return null;
      }
      return parseRawGrammar(fs.readFileSync(grammarPath, 'utf8'), grammarPath);
    },
  });
  const grammar = await registry.loadGrammar('source.atml');
  assert.ok(grammar);
  return grammar;
}
