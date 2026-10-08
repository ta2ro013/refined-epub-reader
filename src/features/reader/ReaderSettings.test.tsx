import { useState } from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { ReaderSettings, defaultSettings } from "./ReaderSettings";

it("6段階の文字サイズを選べ、上限と下限で変更を止める", async () => {
  function Settings() {
    const [value, setValue] = useState(defaultSettings);
    return (
      <ReaderSettings value={value} onChange={setValue} onClose={() => {}} />
    );
  }
  const user = userEvent.setup();
  render(<Settings />);
  const smaller = screen.getByRole("button", { name: "文字を小さく" });
  const larger = screen.getByRole("button", { name: "文字を大きく" });
  const size = screen.getByLabelText("文字サイズ");
  for (const expected of [21, 24, 27]) {
    await user.click(larger);
    expect(size).toHaveTextContent(`${expected}px`);
  }
  expect(larger).toBeDisabled();
  await user.click(larger);
  expect(size).toHaveTextContent("27px");
  for (const expected of [24, 21, 19, 17, 15]) {
    await user.click(smaller);
    expect(size).toHaveTextContent(`${expected}px`);
  }
  expect(smaller).toBeDisabled();
  await user.click(smaller);
  expect(size).toHaveTextContent("15px");
});

it("閉じるボタンとEscapeでパネルを閉じられる", async () => {
  const close = vi.fn();
  const user = userEvent.setup();
  render(
    <ReaderSettings
      value={defaultSettings}
      onChange={() => {}}
      onClose={close}
    />,
  );
  await user.keyboard("{Escape}");
  expect(close).toHaveBeenCalledTimes(1);
  await user.click(screen.getByRole("button", { name: "表示設定を閉じる" }));
  expect(close).toHaveBeenCalledTimes(2);
});
