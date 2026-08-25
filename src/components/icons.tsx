import { FileJson2, FileQuestion, Film, Headphones, Image } from "lucide-react";
import type { ResourceKind } from "../lib/schema";
export const kindLabel: Record<ResourceKind, string> = {
  video: "视频",
  audio: "音频",
  image: "图片",
  data: "数据",
  document: "文档",
  other: "其他",
};
export function KindIcon({
  kind,
  className = "size-4",
}: {
  kind: ResourceKind;
  className?: string;
}) {
  const Icon = {
    video: Film,
    audio: Headphones,
    image: Image,
    data: FileJson2,
    document: FileJson2,
    other: FileQuestion,
  }[kind];
  return <Icon className={className} strokeWidth={1.7} />;
}
