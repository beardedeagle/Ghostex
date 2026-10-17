/**
 * What the font preview shows about a font file: its names, version, weight or weight range,
 * glyph count and the characters it maps, read from the file's own tables. TrueType and OpenType
 * are read directly, WOFF through the browser's zlib decompressor, and WOFF2 through its Brotli
 * decompressor where the browser has one; without it a WOFF2 file still previews, with fewer facts.
 */

export type FontInfo = {
  /** TrueType, OpenType (CFF), WOFF or WOFF2. */
  format: string;
  family?: string;
  subfamily?: string;
  fullName?: string;
  version?: string;
  designer?: string;
  copyright?: string;
  glyphCount?: number;
  weight?: number;
  /** A variable font's weight axis. */
  weightRange?: { min: number; max: number; default: number };
  /** Every other variation axis, by tag. */
  axes: string[];
  /** The characters the font maps, in code point order; `undefined` when they could not be read. */
  codePoints?: number[];
};

type Tables = Map<string, DataView>;

const WOFF2_KNOWN_TAGS = [
  'cmap',
  'head',
  'hhea',
  'hmtx',
  'maxp',
  'name',
  'OS/2',
  'post',
  'cvt ',
  'fpgm',
  'glyf',
  'loca',
  'prep',
  'CFF ',
  'VORG',
  'EBDT',
  'EBLC',
  'gasp',
  'hdmx',
  'kern',
  'LTSH',
  'PCLT',
  'VDMX',
  'vhea',
  'vmtx',
  'BASE',
  'GDEF',
  'GPOS',
  'GSUB',
  'EBSC',
  'JSTF',
  'MATH',
  'CBDT',
  'CBLC',
  'COLR',
  'CPAL',
  'SVG ',
  'sbix',
  'acnt',
  'avar',
  'bdat',
  'bloc',
  'bsln',
  'cvar',
  'fdsc',
  'feat',
  'fmtx',
  'fvar',
  'gvar',
  'hsty',
  'just',
  'lcar',
  'mort',
  'morx',
  'opbd',
  'prop',
  'trak',
  'Zapf',
  'Silf',
  'Glat',
  'Gloc',
  'Feat',
  'Sill',
];
/** Only these tables are read, so a WOFF file decompresses nothing else. */
const WANTED_TABLES = new Set(['name', 'OS/2', 'fvar', 'maxp', 'cmap', 'CFF ', 'CFF2']);
const MAX_CODE_POINTS = 4_000;

function tag(view: DataView, offset: number): string {
  return String.fromCharCode(
    view.getUint8(offset),
    view.getUint8(offset + 1),
    view.getUint8(offset + 2),
    view.getUint8(offset + 3)
  );
}

async function decompress(bytes: Uint8Array<ArrayBuffer>, format: string): Promise<Uint8Array<ArrayBuffer>> {
  const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream(format as CompressionFormat));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

function sfntTables(view: DataView<ArrayBuffer>): Tables {
  const tables: Tables = new Map();
  const count = view.getUint16(4);
  for (let index = 0; index < count; index += 1) {
    const record = 12 + index * 16;
    const name = tag(view, record);
    const offset = view.getUint32(record + 8);
    const length = view.getUint32(record + 12);
    if (WANTED_TABLES.has(name) && offset + length <= view.byteLength) {
      tables.set(name, new DataView(view.buffer, view.byteOffset + offset, length));
    }
  }
  return tables;
}

async function woffTables(view: DataView<ArrayBuffer>): Promise<Tables> {
  const tables: Tables = new Map();
  const count = view.getUint16(12);
  for (let index = 0; index < count; index += 1) {
    const record = 44 + index * 20;
    const name = tag(view, record);
    const offset = view.getUint32(record + 4);
    const compressed = view.getUint32(record + 8);
    const original = view.getUint32(record + 12);
    if (!WANTED_TABLES.has(name) || offset + compressed > view.byteLength) {
      continue;
    }
    const bytes = new Uint8Array(view.buffer, view.byteOffset + offset, compressed);
    const data: Uint8Array<ArrayBuffer> = compressed < original ? await decompress(bytes, 'deflate') : bytes;
    tables.set(name, new DataView(data.buffer, data.byteOffset, data.byteLength));
  }
  return tables;
}

function base128(view: DataView, cursor: { at: number }): number {
  let value = 0;
  for (let index = 0; index < 5; index += 1) {
    const byte = view.getUint8(cursor.at);
    cursor.at += 1;
    value = value * 128 + (byte & 0x7f);
    if ((byte & 0x80) === 0) {
      return value;
    }
  }
  throw new Error('Bad WOFF2 table length.');
}

async function woff2Tables(view: DataView<ArrayBuffer>): Promise<Tables | undefined> {
  const count = view.getUint16(12);
  const compressedSize = view.getUint32(20);
  const cursor = { at: 48 };
  const entries: { name: string; length: number }[] = [];
  for (let index = 0; index < count; index += 1) {
    const flags = view.getUint8(cursor.at);
    cursor.at += 1;
    const tagIndex = flags & 0x3f;
    let name = WOFF2_KNOWN_TAGS[tagIndex] ?? '';
    if (tagIndex === 63) {
      name = tag(view, cursor.at);
      cursor.at += 4;
    }
    const transform = (flags >> 6) & 3;
    const original = base128(view, cursor);
    // glyf and loca are transformed unless their version is 3; every other table only when not 0.
    const transformed = name === 'glyf' || name === 'loca' ? transform !== 3 : transform !== 0;
    const length = transformed ? base128(view, cursor) : original;
    entries.push({ length, name });
  }
  let data: Uint8Array<ArrayBuffer>;
  try {
    data = await decompress(new Uint8Array(view.buffer, view.byteOffset + cursor.at, compressedSize), 'brotli');
  } catch {
    // This browser has no Brotli decompressor.
    return undefined;
  }
  const tables: Tables = new Map();
  let offset = 0;
  for (const { length, name } of entries) {
    if (WANTED_TABLES.has(name) && offset + length <= data.byteLength) {
      tables.set(name, new DataView(data.buffer, data.byteOffset + offset, length));
    }
    offset += length;
  }
  return tables;
}

function readNames(table: DataView | undefined): Map<number, string> {
  const names = new Map<number, string>();
  if (!table || table.byteLength < 6) {
    return names;
  }
  const count = table.getUint16(2);
  const strings = table.getUint16(4);
  // Windows English first, then any Unicode or Windows record, then Mac Roman.
  const rank = (platform: number, encoding: number, language: number) =>
    platform === 3 && language === 0x409
      ? 0
      : platform === 3 || platform === 0
        ? 1
        : platform === 1 && encoding === 0
          ? 2
          : 9;
  const best = new Map<number, number>();
  for (let index = 0; index < count; index += 1) {
    const record = 6 + index * 12;
    if (record + 12 > table.byteLength) {
      break;
    }
    const platform = table.getUint16(record);
    const encoding = table.getUint16(record + 2);
    const language = table.getUint16(record + 4);
    const id = table.getUint16(record + 6);
    const length = table.getUint16(record + 8);
    const offset = strings + table.getUint16(record + 10);
    const score = rank(platform, encoding, language);
    if (score === 9 || score >= (best.get(id) ?? 9) || offset + length > table.byteLength) {
      continue;
    }
    let text = '';
    if (platform === 1) {
      for (let at = 0; at < length; at += 1) {
        text += String.fromCharCode(table.getUint8(offset + at));
      }
    } else {
      for (let at = 0; at + 1 < length; at += 2) {
        text += String.fromCharCode(table.getUint16(offset + at));
      }
    }
    best.set(id, score);
    names.set(id, text.trim());
  }
  return names;
}

function readCodePoints(table: DataView | undefined): number[] | undefined {
  if (!table || table.byteLength < 4) {
    return undefined;
  }
  const count = table.getUint16(2);
  const subtables: { score: number; offset: number }[] = [];
  for (let index = 0; index < count; index += 1) {
    const record = 4 + index * 8;
    const platform = table.getUint16(record);
    const encoding = table.getUint16(record + 2);
    const offset = table.getUint32(record + 4);
    if (offset + 2 > table.byteLength) {
      continue;
    }
    const format = table.getUint16(offset);
    if (format === 12 && (platform === 0 || (platform === 3 && encoding === 10))) {
      subtables.push({ offset, score: 0 });
    } else if (format === 4 && (platform === 0 || (platform === 3 && encoding === 1))) {
      subtables.push({ offset, score: 1 });
    }
  }
  const chosen = subtables.sort((a, b) => a.score - b.score)[0];
  if (!chosen) {
    return undefined;
  }
  const points: number[] = [];
  const at = chosen.offset;
  if (table.getUint16(at) === 12) {
    const groups = table.getUint32(at + 12);
    for (let group = 0; group < groups && points.length < MAX_CODE_POINTS; group += 1) {
      const record = at + 16 + group * 12;
      if (record + 12 > table.byteLength) {
        break;
      }
      const start = table.getUint32(record);
      const end = table.getUint32(record + 4);
      const glyph = table.getUint32(record + 8);
      for (let code = start; code <= end && points.length < MAX_CODE_POINTS; code += 1) {
        if (glyph + (code - start) !== 0) {
          points.push(code);
        }
      }
    }
    return points;
  }
  const segments = table.getUint16(at + 6) / 2;
  const ends = at + 14;
  const starts = ends + segments * 2 + 2;
  const deltas = starts + segments * 2;
  const rangeOffsets = deltas + segments * 2;
  if (rangeOffsets + segments * 2 > table.byteLength) {
    return undefined;
  }
  for (let segment = 0; segment < segments && points.length < MAX_CODE_POINTS; segment += 1) {
    const end = table.getUint16(ends + segment * 2);
    const start = table.getUint16(starts + segment * 2);
    const delta = table.getInt16(deltas + segment * 2);
    const rangeOffsetAt = rangeOffsets + segment * 2;
    const rangeOffset = table.getUint16(rangeOffsetAt);
    for (let code = start; code <= end && code !== 0xffff && points.length < MAX_CODE_POINTS; code += 1) {
      let glyph: number;
      if (rangeOffset === 0) {
        glyph = (code + delta) & 0xffff;
      } else {
        const glyphAt = rangeOffsetAt + rangeOffset + (code - start) * 2;
        glyph = glyphAt + 2 <= table.byteLength ? table.getUint16(glyphAt) : 0;
        glyph = glyph === 0 ? 0 : (glyph + delta) & 0xffff;
      }
      if (glyph !== 0) {
        points.push(code);
      }
    }
  }
  return points;
}

function fixed(table: DataView, offset: number): number {
  return Math.round(table.getInt32(offset) / 65536);
}

/** Reads `bytes` (a whole font file); throws when it is not a font this preview knows. */
export async function readFontInfo(bytes: ArrayBuffer): Promise<FontInfo> {
  const view = new DataView(bytes);
  if (view.byteLength < 12) {
    throw new Error('This file is too short to be a font.');
  }
  const signature = tag(view, 0);
  let tables: Tables | undefined;
  let format: string;
  if (signature === 'wOFF') {
    format = 'WOFF';
    tables = await woffTables(view);
  } else if (signature === 'wOF2') {
    format = 'WOFF2';
    tables = await woff2Tables(view);
  } else if (signature === 'OTTO') {
    format = 'OpenType (CFF)';
    tables = sfntTables(view);
  } else if (view.getUint32(0) === 0x00010000 || signature === 'true') {
    format = 'TrueType';
    tables = sfntTables(view);
  } else {
    throw new Error('This file is not a font this preview can read.');
  }
  const info: FontInfo = { axes: [], format };
  if (!tables) {
    return info;
  }
  if (format === 'WOFF' || format === 'WOFF2') {
    info.format = `${format} (${tables.has('CFF ') || tables.has('CFF2') ? 'OpenType CFF' : 'TrueType'})`;
  }
  const names = readNames(tables.get('name'));
  info.family = names.get(16) || names.get(1);
  info.subfamily = names.get(17) || names.get(2);
  info.fullName = names.get(4);
  info.version = names.get(5)?.replace(/^Version\s+/i, '');
  info.designer = names.get(9) || names.get(8);
  info.copyright = names.get(0);
  const maxp = tables.get('maxp');
  if (maxp && maxp.byteLength >= 6) {
    info.glyphCount = maxp.getUint16(4);
  }
  const os2 = tables.get('OS/2');
  if (os2 && os2.byteLength >= 6) {
    info.weight = os2.getUint16(4);
  }
  const fvar = tables.get('fvar');
  if (fvar && fvar.byteLength >= 16) {
    const axesAt = fvar.getUint16(4);
    const axisCount = fvar.getUint16(8);
    const axisSize = fvar.getUint16(10);
    for (let index = 0; index < axisCount; index += 1) {
      const record = axesAt + index * axisSize;
      if (record + 20 > fvar.byteLength) {
        break;
      }
      const axis = tag(fvar, record);
      if (axis === 'wght') {
        info.weightRange = {
          default: fixed(fvar, record + 8),
          max: fixed(fvar, record + 12),
          min: fixed(fvar, record + 4),
        };
      } else {
        info.axes.push(axis.trim());
      }
    }
  }
  info.codePoints = readCodePoints(tables.get('cmap'));
  return info;
}
