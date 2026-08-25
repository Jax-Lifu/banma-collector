import { describe, expect, it } from "vitest";
import type { ResourceItem } from "./schema";
import {
  groupLogicalResources,
  resourceDisplayExtension,
  resourceMatchesSelection,
  videoVariantLabel,
} from "./resources";

const video = (
  id: string,
  quality: string,
  language = "中文",
): ResourceItem => ({
  id,
  title: "认识月球",
  url: `https://example.test/${id}.mpd`,
  kind: "video",
  extension: "mpd",
  source: "pedia",
  subfolder: "认识月球/课程",
  sequence: 1,
  quality,
  language,
});

describe("资源展示与筛选", () => {
  it("同一视频的不同清晰度只形成一个逻辑资源", () => {
    const groups = groupLogicalResources(
      [video("1080", "1080P"), video("720", "720P")],
      ["全部"],
      ["中文"],
    );
    expect(groups).toHaveLength(1);
    expect(groups[0].variants).toHaveLength(2);
    expect(videoVariantLabel(groups[0].variants)).toContain("1080P");
  });

  it("清晰度和语言可组合筛选，流媒体清单对用户显示为 MP4", () => {
    const english = video("en", "720P", "英文");
    expect(resourceMatchesSelection(english, ["720P"], ["英文"])).toBe(true);
    expect(resourceMatchesSelection(english, ["1080P"], ["英文"])).toBe(false);
    expect(resourceDisplayExtension(english)).toBe("MP4 · 英文 · 720P");
  });
});
