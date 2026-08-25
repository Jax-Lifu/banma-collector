import { describe, expect, it } from "vitest";
import type { ResourceItem } from "./schema";
import {
  filterResourcesForSelection,
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

  it("同组已有明确变体时不下载缺少标签的兜底视频", () => {
    const chinese4k = video("cn-4k", "4K", "中文");
    const english4k = video("en-4k", "4K", "英文");
    const unlabeledLanguage4k = {
      ...video("fallback-4k", "4K"),
      language: null,
    };
    const unlabeledQualityEnglish = {
      ...video("fallback-en", "", "英文"),
      quality: null,
    };

    expect(
      filterResourcesForSelection(
        [unlabeledLanguage4k, english4k, unlabeledQualityEnglish, chinese4k],
        ["4K"],
        ["中文", "英文"],
      ).map((item) => item.id),
    ).toEqual(["en-4k", "cn-4k"]);
  });

  it("整组没有清晰度和语言标签时仍保留唯一默认视频", () => {
    const fallback = {
      ...video("fallback", ""),
      quality: null,
      language: null,
    };
    expect(filterResourcesForSelection([fallback], ["4K"], ["中文"])).toEqual([
      fallback,
    ]);
  });
});
