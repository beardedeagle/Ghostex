import { readFileSync } from 'node:fs';
import { expect, test } from 'vitest';

test('Linux VT archive targets Cargo architecture with a portable CPU', () => {
  const source = readFileSync('apps/desktop/build.rs', 'utf8');
  const linux = source
    .split('if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {')[1]
    ?.split('    if is_windows {')[0];
  expect(linux).toContain('"x86_64-unknown-linux-gnu" => "x86_64-linux-gnu"');
  expect(linux).toContain('"aarch64-unknown-linux-gnu" => "aarch64-linux-gnu"');
  expect(linux).toContain('command.arg(format!("-Dtarget={zig_target}"))');
  expect(linux).toContain('command.arg("-Dcpu=baseline")');
});
