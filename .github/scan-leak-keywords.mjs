import { readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';

const rawKeywords = process.env.LEAK_WORDS;
if (!rawKeywords) {
  throw new Error('LEAK_WORDS must contain a JSON array of keywords');
}

let keywords;
try {
  keywords = JSON.parse(rawKeywords);
} catch {
  throw new Error('LEAK_WORDS must be valid JSON');
}

if (!Array.isArray(keywords) || keywords.some((keyword) => typeof keyword !== 'string' || !keyword)) {
  throw new Error('LEAK_WORDS must be a JSON array of non-empty strings');
}

const files = spawnSync('git', ['ls-files', '-z'], { encoding: 'buffer' });
if (files.status !== 0) {
  throw new Error(`git ls-files failed: ${files.stderr.toString()}`);
}

let found = false;
for (const file of files.stdout.toString().split('\0').filter(Boolean)) {
  let content;
  try {
    content = readFileSync(file, 'utf8');
  } catch {
    continue;
  }

  for (const [lineOffset, line] of content.split(/\r?\n/).entries()) {
    const normalizedLine = line.toLowerCase();
    for (const keyword of keywords) {
      const normalizedKeyword = keyword.toLowerCase();
      let index = normalizedLine.indexOf(normalizedKeyword);
      while (index !== -1) {
        console.error(JSON.stringify({ file, line: lineOffset + 1, index, content: line, keyword }));
        found = true;
        index = normalizedLine.indexOf(normalizedKeyword, index + normalizedKeyword.length);
      }
    }
  }
}

if (found) {
  process.exitCode = 1;
}
