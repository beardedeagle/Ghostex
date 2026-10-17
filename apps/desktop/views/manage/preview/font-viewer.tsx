import { IconAlertTriangle } from '@tabler/icons-react';
import { useEffect, useState } from 'react';
import { type FontInfo, readFontInfo } from './font-info';
import { ManagePreviewMessage } from './preview-shared';

const SAMPLE_SIZES = [12, 16, 24, 36, 48, 72];
const DEFAULT_SAMPLE = 'The quick brown fox jumps over the lazy dog 0123456789';
/** Shown when the font's character map could not be read (a WOFF2 file without Brotli). */
const FALLBACK_CHARACTERS = Array.from({ length: 0x7e - 0x20 }, (_, index) => 0x21 + index);
const WEIGHT_NAMES: Record<number, string> = {
  100: 'Thin',
  200: 'Extra Light',
  300: 'Light',
  400: 'Regular',
  500: 'Medium',
  600: 'Semibold',
  700: 'Bold',
  800: 'Extra Bold',
  900: 'Black',
};

function isShownCharacter(code: number): boolean {
  return code > 0x20 && !(code >= 0x7f && code <= 0xa0) && !(code >= 0xd800 && code <= 0xdfff);
}

function hex(code: number): string {
  return `U+${code.toString(16).toUpperCase().padStart(4, '0')}`;
}

/**
 * CDXC:Docs 2026-10-17 DECISION:
 * User (Q228) picked the font "preview page": a font file in Files shows its name and version, sample text at several sizes, its weights and a grid of its characters, drawn by the embed page with the browser loading the font, never installing it.
 */
export function ManageFontViewer({
  fileName,
  revision,
  source,
}: {
  fileName: string;
  revision: string;
  source: string;
}) {
  const [family, setFamily] = useState<string>();
  const [info, setInfo] = useState<FontInfo>();
  const [error, setError] = useState<string>();
  const [sample, setSample] = useState(DEFAULT_SAMPLE);

  useEffect(() => {
    let cancelled = false;
    let face: FontFace | undefined;
    setError(undefined);
    setFamily(undefined);
    setInfo(undefined);
    (async () => {
      const response = await fetch(source);
      if (!response.ok) {
        throw new Error("This font can't be read.");
      }
      const bytes = await response.arrayBuffer();
      const name = `ghostex-font-preview-${revision}-${Math.random().toString(36).slice(2)}`;
      face = new FontFace(name, bytes);
      await face.load();
      if (cancelled) {
        return;
      }
      document.fonts.add(face);
      setFamily(name);
      setInfo(await readFontInfo(bytes).catch(() => ({ axes: [], format: 'Font' })));
    })().catch((reason: unknown) => {
      if (!cancelled) {
        setError(reason instanceof Error && reason.message ? reason.message : "This font can't be shown.");
      }
    });
    return () => {
      cancelled = true;
      if (face) {
        document.fonts.delete(face);
      }
    };
  }, [revision, source]);

  if (error) {
    return <ManagePreviewMessage icon={<IconAlertTriangle aria-hidden size={16} />} title={error} />;
  }
  if (!family || !info) {
    return null;
  }
  const fontFamily = `"${family}", sans-serif`;
  const title = info.fullName || [info.family, info.subfamily].filter(Boolean).join(' ') || fileName;
  const weights = info.weightRange
    ? [100, 200, 300, 400, 500, 600, 700, 800, 900].filter(
        (weight) => weight >= info.weightRange!.min && weight <= info.weightRange!.max
      )
    : [];
  const facts = [
    info.subfamily,
    info.version && `Version ${info.version}`,
    info.format,
    info.glyphCount !== undefined && `${info.glyphCount.toLocaleString()} glyphs`,
    info.weightRange
      ? `Variable weight ${info.weightRange.min}–${info.weightRange.max}`
      : info.weight !== undefined &&
        `Weight ${info.weight}${WEIGHT_NAMES[info.weight] ? ` (${WEIGHT_NAMES[info.weight]})` : ''}`,
    info.axes.length > 0 && `Axes: ${info.axes.join(', ')}`,
  ].filter(Boolean) as string[];
  const characters = (info.codePoints ?? FALLBACK_CHARACTERS).filter(isShownCharacter);

  return (
    <div className='manage-embed-font'>
      <header className='manage-font-header'>
        <h1 className='manage-font-title' style={{ fontFamily }}>
          {title}
        </h1>
        <div className='manage-font-facts'>
          {facts.map((fact) => (
            <span key={fact}>{fact}</span>
          ))}
        </div>
        {info.designer || info.copyright ? (
          <div className='manage-font-credit'>{[info.designer, info.copyright].filter(Boolean).join(' · ')}</div>
        ) : null}
      </header>

      <section className='manage-font-section manage-font-samples'>
        <input
          aria-label='Sample text'
          className='manage-font-sample-input'
          onChange={(event) => setSample(event.target.value)}
          placeholder='Type sample text'
          spellCheck={false}
          value={sample}
        />
        {SAMPLE_SIZES.map((size) => (
          <div className='manage-font-sample-row' key={size}>
            <span className='manage-font-sample-size'>{size}</span>
            <div className='manage-font-sample-text' style={{ fontFamily, fontSize: size }}>
              {sample || DEFAULT_SAMPLE}
            </div>
          </div>
        ))}
      </section>

      {weights.length > 1 ? (
        <section className='manage-font-section manage-font-weights'>
          <h2 className='manage-font-section-title'>Weights</h2>
          {weights.map((weight) => (
            <div className='manage-font-sample-row' key={weight}>
              <span className='manage-font-sample-size'>{weight}</span>
              <div className='manage-font-sample-text' style={{ fontFamily, fontSize: 24, fontWeight: weight }}>
                {sample || DEFAULT_SAMPLE}
              </div>
            </div>
          ))}
        </section>
      ) : null}

      <section className='manage-font-section manage-font-glyphs'>
        <h2 className='manage-font-section-title'>
          Characters
          <span>
            {info.codePoints
              ? `${characters.length.toLocaleString()}${info.codePoints.length >= 4_000 ? '+' : ''}`
              : "Basic Latin shown: this file's character list can't be read here"}
          </span>
        </h2>
        <div className='manage-font-glyph-grid'>
          {characters.map((code) => (
            <div className='manage-font-glyph' key={code} title={hex(code)}>
              <span className='manage-font-glyph-char' style={{ fontFamily }}>
                {String.fromCodePoint(code)}
              </span>
              <span className='manage-font-glyph-code'>{hex(code)}</span>
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}
