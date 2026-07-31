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
