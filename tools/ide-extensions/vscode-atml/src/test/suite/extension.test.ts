import * as assert from 'node:assert';
import * as os from 'node:os';
import * as path from 'node:path';
import * as vscode from 'vscode';

suite('ATML extension', () => {
  test('activates for .atml and receives server diagnostics', async () => {
    const uri = vscode.Uri.file(
      path.join(os.tmpdir(), `atml-extension-${process.pid}-${Date.now()}.atml`),
    );
    await vscode.workspace.fs.writeFile(uri, Buffer.from('broken = [\n'));

    try {
      const document = await vscode.workspace.openTextDocument(uri);
      await vscode.window.showTextDocument(document);
      assert.equal(document.languageId, 'atml');

      const extension = vscode.extensions.getExtension('kathrinmotzkus.atml');
      assert.ok(extension, 'extension is installed in the development host');
      await extension.activate();
      assert.equal(extension.isActive, true);

      const diagnostics = await waitForDiagnostics(uri, (items) => items.length > 0);
      assert.equal(diagnostics[0].source, 'atml');
      assert.equal(diagnostics[0].code, 'atml.syntax.parse-error');

      const edit = new vscode.WorkspaceEdit();
      const fullRange = new vscode.Range(
        document.positionAt(0),
        document.positionAt(document.getText().length),
      );
      edit.replace(uri, fullRange, 'value = 1\n');
      assert.equal(await vscode.workspace.applyEdit(edit), true);
      await document.save();
      await waitForDiagnostics(uri, (items) => items.length === 0);

      const completionSource = 'Strategy[] = [Active, Passive]\nchoice = Strategy::A\n';
      const completionEdit = new vscode.WorkspaceEdit();
      completionEdit.replace(
        uri,
        new vscode.Range(document.positionAt(0), document.positionAt(document.getText().length)),
        completionSource,
      );
      assert.equal(await vscode.workspace.applyEdit(completionEdit), true);
      const completions = await waitForCompletions(uri, new vscode.Position(1, 20));
      const active = completions.items.find((item) => item.label === 'Active');
      assert.ok(active, 'enum member completion is provided by the language server');
      assert.ok(active.textEdit instanceof vscode.TextEdit);
      assert.equal(active.textEdit.newText, 'Active');
      assert.deepEqual(active.textEdit.range, new vscode.Range(1, 19, 1, 20));

      const navigationSource =
        'Mode[] = [Active, Passive]\n[root]\nspeed = 5m²\n[child : root]\n' +
        'mode = Mode::Active\ncopy = root.speed\n';
      const navigationEdit = new vscode.WorkspaceEdit();
      navigationEdit.replace(
        uri,
        new vscode.Range(document.positionAt(0), document.positionAt(document.getText().length)),
        navigationSource,
      );
      assert.equal(await vscode.workspace.applyEdit(navigationEdit), true);

      const hovers = await waitForCommand<vscode.Hover[]>(
        'vscode.executeHoverProvider',
        [uri, new vscode.Position(5, 12)],
        (items) => items.length > 0,
      );
      assert.ok(
        hovers[0].contents.some((content) => {
          const value = typeof content === 'string' ? content : content.value;
          return value.includes('Resolved value: `5m²`');
        }),
      );

      const definitions = await waitForCommand<(vscode.Location | vscode.LocationLink)[]>(
        'vscode.executeDefinitionProvider',
        [uri, new vscode.Position(5, 12)],
        (items) => items.length > 0,
      );
      const definition = definitions[0];
      assert.ok(definition instanceof vscode.Location);
      assert.deepEqual(definition.range, new vscode.Range(2, 0, 2, 5));

      const references = await waitForCommand<vscode.Location[]>(
        'vscode.executeReferenceProvider',
        [uri, new vscode.Position(2, 2)],
        (items) => items.length > 0,
      );
      assert.ok(
        references.some((location) => location.range.isEqual(new vscode.Range(5, 7, 5, 17))),
      );

      const semanticTokens = await waitForCommand<vscode.SemanticTokens>(
        'vscode.provideDocumentSemanticTokens',
        [uri],
        (tokens) => tokens.data.length > 0,
      );
      assert.ok(semanticTokens.data.length > 0);

      const renameEdit = await waitForCommand<vscode.WorkspaceEdit>(
        'vscode.executeDocumentRenameProvider',
        [uri, new vscode.Position(2, 2), 'velocity'],
        (edit) => edit.entries().length > 0,
      );
      const renameRanges = renameEdit.entries().flatMap(([, edits]) =>
        edits.map((edit) => edit.range),
      );
      assert.ok(renameRanges.some((range) => range.isEqual(new vscode.Range(2, 0, 2, 5))));
      assert.ok(renameRanges.some((range) => range.isEqual(new vscode.Range(5, 12, 5, 17))));

      const invalidEdit = new vscode.WorkspaceEdit();
      invalidEdit.replace(
        uri,
        new vscode.Range(document.positionAt(0), document.positionAt(document.getText().length)),
        'Mode[] = [Active]\nvalue = Mode::active\n',
      );
      assert.equal(await vscode.workspace.applyEdit(invalidEdit), true);
      await waitForDiagnostics(uri, (items) =>
        items.some((diagnostic) => diagnostic.code === 'atml.enum.unknown-member'),
      );
      const actions = await waitForCommand<(vscode.CodeAction | vscode.Command)[]>(
        'vscode.executeCodeActionProvider',
        [uri, new vscode.Range(1, 8, 1, 20), vscode.CodeActionKind.QuickFix.value],
        (items) => items.some((item) => item.title === "Change to 'Active'"),
      );
      const fix = actions.find((item) => item.title === "Change to 'Active'");
      assert.ok(fix && 'edit' in fix && fix.edit);
    } finally {
      await vscode.commands.executeCommand('workbench.action.closeActiveEditor');
      await vscode.workspace.fs.delete(uri, { useTrash: false });
    }
  });
});

async function waitForDiagnostics(
  uri: vscode.Uri,
  predicate: (diagnostics: readonly vscode.Diagnostic[]) => boolean,
): Promise<readonly vscode.Diagnostic[]> {
  const deadline = Date.now() + 20_000;
  while (Date.now() < deadline) {
    const diagnostics = vscode.languages.getDiagnostics(uri);
    if (predicate(diagnostics)) {
      return diagnostics;
    }
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(`timed out waiting for diagnostics for ${uri.toString()}`);
}

async function waitForCompletions(
  uri: vscode.Uri,
  position: vscode.Position,
): Promise<vscode.CompletionList> {
  const deadline = Date.now() + 20_000;
  while (Date.now() < deadline) {
    const completions = await vscode.commands.executeCommand<vscode.CompletionList>(
      'vscode.executeCompletionItemProvider',
      uri,
      position,
    );
    if (completions?.items.some((item) => item.label === 'Active')) {
      return completions;
    }
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(`timed out waiting for completions for ${uri.toString()}`);
}

async function waitForCommand<T>(
  command: string,
  args: unknown[],
  predicate: (result: T) => boolean,
): Promise<T> {
  const deadline = Date.now() + 20_000;
  while (Date.now() < deadline) {
    const result = await vscode.commands.executeCommand<T>(command, ...args);
    if (result && predicate(result)) {
      return result;
    }
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(`timed out waiting for ${command}`);
}
