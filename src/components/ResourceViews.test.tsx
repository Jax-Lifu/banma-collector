import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { MediaRow } from "./ResourceViews";
import type { ResourceItem } from "../lib/schema";

const resource: ResourceItem = {
  id: "moon-video",
  title: "认识月球",
  url: "https://example.test/moon.mpd",
  kind: "video",
  extension: "mpd",
  source: "pedia",
  subfolder: "认识月球/课程",
  quality: "1080P",
  language: "中文",
};

describe("资源行", () => {
  it("展示统一名称、变体和状态，并支持选择", () => {
    const onToggle = vi.fn();
    render(
      <MediaRow
        resource={resource}
        selected
        state={{ status: "queued", received: 0 }}
        onToggle={onToggle}
        onCancel={vi.fn()}
        onRetry={vi.fn()}
        onPreview={vi.fn()}
        accent="#f05a38"
        variantLabel="中文：1080P / 720P"
      />,
    );
    expect(screen.getByText("认识月球")).toBeInTheDocument();
    expect(screen.getByText("MP4 · 中文：1080P / 720P")).toBeInTheDocument();
    expect(screen.getByText("排队中")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "取消选择" }));
    expect(onToggle).toHaveBeenCalledOnce();
  });
});
