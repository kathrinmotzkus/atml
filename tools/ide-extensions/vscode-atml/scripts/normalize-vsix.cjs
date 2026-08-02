'use strict';

const fs = require('node:fs');
const path = require('node:path');
const yauzl = require('yauzl');
const yazl = require('yazl');

const [artifact, epochText] = process.argv.slice(2);
if (!artifact || !epochText || !/^\d+$/.test(epochText)) {
  throw new Error('usage: node normalize-vsix.cjs <vsix> <source-date-epoch>');
}

const timestamp = new Date(Number(epochText) * 1000);
const entries = [];

yauzl.open(artifact, { lazyEntries: true }, (openError, archive) => {
  if (openError) throw openError;
  archive.readEntry();
  archive.on('entry', (entry) => {
    if (entry.fileName.endsWith('/')) {
      archive.readEntry();
      return;
    }
    archive.openReadStream(entry, (streamError, stream) => {
      if (streamError) throw streamError;
      const chunks = [];
      stream.on('data', (chunk) => chunks.push(chunk));
      stream.on('end', () => {
        entries.push({
          name: entry.fileName,
          data: Buffer.concat(chunks),
          mode: (entry.externalFileAttributes >>> 16) & 0xffff,
        });
        archive.readEntry();
      });
    });
  });
  archive.on('end', () => writeNormalized(entries));
});

function writeNormalized(items) {
  items.sort((left, right) => (left.name < right.name ? -1 : left.name > right.name ? 1 : 0));
  const output = new yazl.ZipFile();
  for (const item of items) {
    output.addBuffer(item.data, item.name, {
      mtime: timestamp,
      mode: item.mode || 0o100644,
      compress: true,
    });
  }
  const temporary = `${artifact}.normalized`;
  output.outputStream.pipe(fs.createWriteStream(temporary)).on('close', () => {
    fs.renameSync(temporary, artifact);
    process.stdout.write(`normalized ${path.basename(artifact)}\n`);
  });
  output.end();
}
