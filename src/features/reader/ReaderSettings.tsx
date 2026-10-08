import { useEffect, useRef } from "react";

export type ReaderTheme = "paper" | "kinari" | "night";
export type ReaderFont = "mincho" | "gothic";
export type ReadingSettings = {
  theme: ReaderTheme;
  fontSize: number;
  fontFamily: ReaderFont;
};
export const fontSizes = [15, 17, 19, 21, 24, 27] as const;
export const defaultSettings: ReadingSettings = {
  theme: "paper",
  fontSize: 19,
  fontFamily: "mincho",
};
export const fontStacks = {
  mincho: '"Shippori Mincho B1", "Hiragino Mincho ProN", "Yu Mincho", serif',
  gothic: '"Zen Kaku Gothic New", "Hiragino Sans", "Yu Gothic", sans-serif',
};
const themes = [
  { id: "paper", label: "紙" },
  { id: "kinari", label: "生成" },
  { id: "night", label: "夜" },
] as const;

export function ReaderSettings({
  value,
  onChange,
  onClose,
}: {
  value: ReadingSettings;
  onChange: (value: ReadingSettings) => void;
  onClose: () => void;
}) {
  const first = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    first.current?.focus();
  }, []);
  const step = fontSizes.findIndex((size) => size === value.fontSize);
  return (
    <section
      id="reader-settings"
      className="reader-settings"
      role="dialog"
      aria-label="表示設定"
      onKeyDown={(event) => {
        event.stopPropagation();
        if (event.key === "Escape") {
          event.preventDefault();
          onClose();
        }
      }}
    >
      <div className="settings-heading">
        <h2>表示設定</h2>
        <button
          type="button"
          className="settings-close"
          aria-label="表示設定を閉じる"
          onClick={onClose}
        >
          閉じる
        </button>
      </div>
      <fieldset>
        <legend>テーマ</legend>
        <div className="theme-options">
          {themes.map(({ id, label }, index) => (
            <button
              ref={index === 0 ? first : undefined}
              key={id}
              type="button"
              className="theme-choice"
              data-theme={id}
              aria-pressed={value.theme === id}
              onClick={() => onChange({ ...value, theme: id })}
            >
              <span aria-hidden="true">あ</span>
              {label}
            </button>
          ))}
        </div>
      </fieldset>
      <fieldset>
        <legend>文字サイズ</legend>
        <div className="size-options">
          <button
            type="button"
            aria-label="文字を小さく"
            disabled={step <= 0}
            onClick={() =>
              onChange({ ...value, fontSize: fontSizes[step - 1] })
            }
          >
            あ
          </button>
          <div className="size-steps" aria-hidden="true">
            {fontSizes.map((size, index) => (
              <span
                key={size}
                className={size === value.fontSize ? "selected" : ""}
                style={{ width: 6 + index * 2, height: 6 + index * 2 }}
              />
            ))}
          </div>
          <button
            type="button"
            aria-label="文字を大きく"
            disabled={step >= fontSizes.length - 1}
            onClick={() =>
              onChange({ ...value, fontSize: fontSizes[step + 1] })
            }
          >
            あ
          </button>
        </div>
        <output
          className="size-value"
          aria-label="文字サイズ"
          aria-live="polite"
        >
          {value.fontSize}px
        </output>
      </fieldset>
      <fieldset>
        <legend>書体</legend>
        <div className="font-options">
          {(
            [
              { id: "mincho", label: "明朝" },
              { id: "gothic", label: "ゴシック" },
            ] as const
          ).map(({ id, label }) => (
            <button
              key={id}
              type="button"
              aria-pressed={value.fontFamily === id}
              style={{ fontFamily: fontStacks[id] }}
              onClick={() => onChange({ ...value, fontFamily: id })}
            >
              {label}
            </button>
          ))}
        </div>
      </fieldset>
    </section>
  );
}
