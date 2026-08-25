import type { ProgressState, ResourceItem } from "./schema";

export function kindName(kind: ResourceItem["kind"]) {
  return (
    {
      video: "视频",
      audio: "音频",
      image: "图片",
      document: "文档",
      data: "数据",
      other: "其他",
    } as const
  )[kind];
}

export function resourceDisplayExtension(resource: ResourceItem) {
  const extension = resource.extension.toLowerCase();
  const format =
    resource.kind === "video" && (extension === "mpd" || extension === "m3u8")
      ? "MP4"
      : resource.kind === "audio" &&
          (extension === "mpd" || extension === "m3u8")
        ? "MP3"
        : (
            resource.extension ||
            (resource.kind === "audio"
              ? "MP3"
              : resource.kind === "video"
                ? "MP4"
                : "MEDIA")
          ).toUpperCase();
  const attributes = [resource.language, resource.quality].filter(Boolean);
  return attributes.length ? `${format} · ${attributes.join(" · ")}` : format;
}

export function resourceDisplayTitle(resource: ResourceItem) {
  if (resource.kind !== "video") return resource.title;
  const title = resource.title.replace(/\.(mpd|m3u8|mp4|mov|webm|mkv)$/i, "");
  const uuid =
    /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(
      title,
    );
  if (!uuid && title.trim() !== "课程视频") return title;
  return resource.subfolder?.split("/").filter(Boolean).at(-1) || "课程视频";
}

export function resourceMatchesSelection(
  resource: ResourceItem,
  selectedQualities: string[],
  selectedLanguages: string[],
) {
  const matchesQuality =
    resource.kind !== "video" ||
    !resource.quality ||
    selectedQualities.includes("全部") ||
    selectedQualities.includes(resource.quality);
  if (!matchesQuality) return false;
  if (
    resource.kind !== "video" ||
    !resource.language ||
    selectedLanguages.includes("全部")
  )
    return true;
  return selectedLanguages.includes(resource.language);
}

function logicalResourceKey(resource: ResourceItem) {
  if (resource.kind !== "video" || (!resource.quality && !resource.language))
    return resource.id;
  return [
    resource.source,
    resource.subfolder || "",
    resource.sequence || 0,
    resourceDisplayTitle(resource),
  ].join("|");
}

function qualitySort(
  left: string | null | undefined,
  right: string | null | undefined,
) {
  const order = ["4K", "1080P", "720P", "标清"];
  return order.indexOf(left || "") - order.indexOf(right || "");
}

export function videoVariantLabel(variants: ResourceItem[]) {
  const languages = [
    ...new Set(
      variants
        .map((item) => item.language)
        .filter((value): value is string => Boolean(value)),
    ),
  ];
  if (!languages.length) {
    const qualities = [
      ...new Set(
        variants
          .map((item) => item.quality)
          .filter((value): value is string => Boolean(value)),
      ),
    ].sort(qualitySort);
    return qualities.length > 1 ? qualities.join(" / ") : qualities[0];
  }
  return languages
    .map((language) => {
      const qualities = [
        ...new Set(
          variants
            .filter((item) => item.language === language)
            .map((item) => item.quality)
            .filter((value): value is string => Boolean(value)),
        ),
      ].sort(qualitySort);
      return qualities.length
        ? `${language}：${qualities.join(" / ")}`
        : `${language}：默认`;
    })
    .join("；");
}

export function groupLogicalResources(
  resources: ResourceItem[],
  selectedQualities: string[],
  selectedLanguages: string[],
) {
  const groups = new Map<string, ResourceItem[]>();
  for (const resource of resources) {
    const key = logicalResourceKey(resource);
    groups.set(key, [...(groups.get(key) ?? []), resource]);
  }
  return [...groups.values()].map((variants) => ({
    variants,
    resource:
      variants.find((item) =>
        resourceMatchesSelection(item, selectedQualities, selectedLanguages),
      ) ?? variants[0],
  }));
}

export function progressStatusText(state?: ProgressState) {
  if (!state) return "尚未下载";
  return (
    {
      queued: "等待下载",
      downloading: "正在下载",
      completed: "下载完成",
      failed: "下载失败",
      cancelled: "已取消",
    } as const
  )[state.status];
}

export function readError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export function resourceDownloadPriority(item: ResourceItem) {
  return (
    (
      {
        video: 0,
        audio: 1,
        document: 2,
        image: 3,
        data: 4,
        other: 5,
      } as Record<string, number>
    )[item.kind] ?? 6
  );
}

export function isLoginError(error: unknown) {
  const value = readError(error);
  return (
    value.includes("LOGIN_REQUIRED") ||
    value.includes("401") ||
    value.includes("登录")
  );
}
