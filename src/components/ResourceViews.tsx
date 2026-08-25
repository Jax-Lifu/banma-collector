import { useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import {
  Check,
  Download,
  Eye,
  FileCode,
  FileText,
  Headphones,
  Image as ImageIcon,
  LoaderCircle,
  Music,
  Play,
  RotateCcw,
  X,
} from "lucide-react";
import type { ProgressState, ResourceItem } from "../lib/schema";
import { getResourcePreviewPath } from "../lib/tauri";
import { formatBytes } from "../lib/utils";
import {
  kindName,
  progressStatusText,
  resourceDisplayExtension,
  resourceDisplayTitle,
} from "../lib/resources";
import { cn } from "../lib/utils";
import { Button } from "./ui/button";

export type AlbumProgressGroup = {
  name: string;
  total: number;
  completed: number;
  downloading: number;
  failed: number;
  percent: number;
};

export function buildAlbumProgress(
  resources: ResourceItem[],
  progress: Record<string, ProgressState>,
): AlbumProgressGroup[] {
  const groups = new Map<
    string,
    {
      total: number;
      completed: number;
      downloading: number;
      failed: number;
      value: number;
    }
  >();
  for (const resource of resources) {
    const state = progress[resource.id];
    if (!state) continue;
    const name =
      resource.subfolder?.split(/[\\/]/).filter(Boolean)[0] || "未分类资源";
    const group = groups.get(name) ?? {
      total: 0,
      completed: 0,
      downloading: 0,
      failed: 0,
      value: 0,
    };
    group.total += 1;
    if (state.status === "completed") {
      group.completed += 1;
      group.value += 1;
    }
    if (state.status === "downloading") {
      group.downloading += 1;
      group.value +=
        state.total && state.total > 0
          ? Math.min(1, state.received / state.total)
          : 0.08;
    }
    if (state.status === "failed" || state.status === "cancelled")
      group.failed += 1;
    groups.set(name, group);
  }
  return [...groups.entries()]
    .map(([name, group]) => ({
      name,
      total: group.total,
      completed: group.completed,
      downloading: group.downloading,
      failed: group.failed,
      percent: group.total ? Math.round((group.value / group.total) * 100) : 0,
    }))
    .sort(
      (a, b) =>
        Number(b.downloading > 0) - Number(a.downloading > 0) ||
        a.name.localeCompare(b.name, "zh-CN"),
    );
}

export function AlbumProgressPanel({
  groups,
  preparing,
  accent,
}: {
  groups: AlbumProgressGroup[];
  preparing: string[];
  accent: string;
}) {
  const completedAlbums = groups.filter(
    (group) => group.completed === group.total,
  ).length;
  return (
    <section className="download-overview mb-8 overflow-hidden rounded-2xl border border-black/[.08] bg-white">
      <div className="flex items-center justify-between border-b border-black/[.07] px-5 py-4">
        <div>
          <h2 className="text-sm font-semibold">专辑下载进度</h2>
          <p className="mt-1 text-[11px] text-black/40">
            按保存目录汇总，每个专辑的完成情况一目了然
          </p>
        </div>
        <span className="rounded-full bg-black/[.05] px-2.5 py-1 text-[10px] font-medium text-black/55">
          {preparing.length
            ? `解析中 · ${preparing.length}`
            : `${completedAlbums}/${groups.length} 个专辑`}
        </span>
      </div>
      <div className="album-progress-list max-h-64 overflow-y-auto">
        {preparing.map((name) => (
          <div
            key={name}
            className="grid grid-cols-[minmax(0,1fr)_100px] items-center gap-5 border-b border-black/[.055] px-5 py-3 last:border-0"
          >
            <div className="min-w-0">
              <p className="truncate text-xs font-medium">{name}</p>
              <p className="mt-1 text-[10px] text-black/35">
                正在读取专辑与全部子课程…
              </p>
            </div>
            <div className="h-1.5 overflow-hidden rounded-full bg-black/[.06]">
              <span
                className="progress-indeterminate block h-full rounded-full"
                style={{ background: accent }}
              />
            </div>
          </div>
        ))}
        {groups.map((group) => (
          <div
            key={group.name}
            className="album-progress-row grid grid-cols-[minmax(0,1fr)_170px_48px] items-center gap-5 border-b border-black/[.055] px-5 py-3.5 last:border-0"
          >
            <div className="min-w-0">
              <p className="truncate text-xs font-semibold">{group.name}</p>
              <p className="mt-1 text-[10px] text-black/38">
                {group.downloading
                  ? `${group.downloading} 个正在下载`
                  : group.completed === group.total
                    ? "已全部完成"
                    : `${group.completed}/${group.total} 个已完成`}
                {group.failed ? ` · ${group.failed} 个异常` : ""}
              </p>
            </div>
            <div className="h-1.5 overflow-hidden rounded-full bg-black/[.065]">
              <span
                className="block h-full rounded-full transition-[width] duration-500"
                style={{
                  width: `${group.percent}%`,
                  background: group.failed ? "#e67e52" : accent,
                }}
              />
            </div>
            <span
              className={cn(
                "text-right text-[11px] font-semibold tabular-nums",
                group.failed ? "text-red-600" : "text-black/55",
              )}
            >
              {group.percent}%
            </span>
          </div>
        ))}
      </div>
    </section>
  );
}

export function MediaRow({
  resource,
  selected,
  state,
  onToggle,
  onCancel,
  onRetry,
  onPreview,
  accent,
  variantLabel,
}: {
  resource: ResourceItem;
  selected: boolean;
  state?: ProgressState;
  onToggle: () => void;
  onCancel: (e: React.MouseEvent) => void;
  onRetry: (e: React.MouseEvent) => void;
  onPreview: () => void;
  accent: string;
  variantLabel?: string;
}) {
  const tag = variantLabel
    ? `MP4 · ${variantLabel}`
    : resourceDisplayExtension(resource);
  const title = resourceDisplayTitle(resource);
  const Icon =
    resource.kind === "audio"
      ? Music
      : resource.kind === "video"
        ? Play
        : resource.kind === "image"
          ? ImageIcon
          : resource.kind === "document"
            ? FileText
            : FileCode;
  return (
    <div
      onClick={onPreview}
      className="media-row grid w-full cursor-pointer grid-cols-[32px_36px_minmax(0,1fr)_100px_32px] items-center border-b border-black/[.06] px-4 py-3 text-left transition-colors last:border-0 hover:bg-black/[.025]"
    >
      <button
        onClick={(event) => {
          event.stopPropagation();
          onToggle();
        }}
        aria-label={selected ? "取消选择" : "选择资源"}
        className={cn(
          "grid size-4 place-items-center rounded border",
          selected ? "text-white" : "border-black/20",
        )}
        style={
          selected ? { background: accent, borderColor: accent } : undefined
        }
      >
        {selected && <Check className="size-3" />}
      </button>
      <span className="grid size-8 place-items-center rounded-md bg-black/[.045]">
        <Icon
          className={cn(
            "size-3.5",
            resource.kind === "audio"
              ? "text-purple-600"
              : resource.kind === "video"
                ? "text-blue-600"
                : resource.kind === "image"
                  ? "text-emerald-600"
                  : resource.kind === "document"
                    ? "text-amber-600"
                    : "text-gray-600",
          )}
        />
      </span>
      <span className="min-w-0 px-2">
        <span className="block truncate text-xs font-medium">{title}</span>
        <span className="flex items-center gap-1.5 truncate pt-1 text-[10px] text-black/35">
          {resource.subfolder && (
            <span className="rounded bg-black/[.06] px-1.5 py-0.5 font-medium text-black/60">
              {resource.subfolder}
            </span>
          )}
          <span>{tag}</span>
          {resource.size ? <span>· {formatBytes(resource.size)}</span> : null}
        </span>
      </span>
      <div className="flex items-center justify-end text-right text-[10px]">
        {state?.status === "completed" ? (
          <span className="font-medium text-emerald-600">已完成</span>
        ) : state?.status === "downloading" ? (
          <div className="flex items-center text-blue-600">
            <span>
              {state.total && state.total > 0
                ? `${Math.floor((state.received / state.total) * 100)}%`
                : "下载中"}
            </span>
            <button
              onClick={onCancel}
              title="取消下载"
              className="ml-1.5 rounded p-0.5 text-black/40 hover:bg-black/10 hover:text-red-600"
            >
              <X className="size-3" />
            </button>
          </div>
        ) : state?.status === "failed" || state?.status === "cancelled" ? (
          <div
            className={
              state.status === "failed"
                ? "flex items-center text-red-500"
                : "flex items-center text-amber-600"
            }
          >
            <span>{state.status === "failed" ? "失败" : "已取消"}</span>
            <button
              onClick={onRetry}
              title="重新下载"
              className="ml-1 rounded p-0.5 text-black/40 hover:bg-black/10 hover:text-black"
            >
              <RotateCcw className="size-3" />
            </button>
          </div>
        ) : state?.status === "queued" ? (
          <span className="text-black/40">排队中</span>
        ) : (
          <span className="text-black/40">待下载</span>
        )}
      </div>
      <button
        onClick={(event) => {
          event.stopPropagation();
          onPreview();
        }}
        title="预览资源"
        className="grid size-7 place-items-center rounded-md text-black/30 transition-colors hover:bg-black/[.06] hover:text-black"
      >
        <Eye className="size-3.5" />
      </button>
    </div>
  );
}

export function ResourcePreview({
  resource,
  outputDir,
  state,
  accent,
  onClose,
  onDownload,
}: {
  resource: ResourceItem;
  outputDir: string;
  state?: ProgressState;
  accent: string;
  onClose: () => void;
  onDownload: () => void;
}) {
  const [localPath, setLocalPath] = useState<string>();
  const [checking, setChecking] = useState(true);
  useEffect(() => {
    let active = true;
    setChecking(true);
    getResourcePreviewPath(resource, outputDir)
      .then((path) => {
        if (active) setLocalPath(path ?? undefined);
      })
      .catch(() => {
        if (active) setLocalPath(undefined);
      })
      .finally(() => {
        if (active) setChecking(false);
      });
    return () => {
      active = false;
    };
  }, [resource, outputDir, state?.status]);
  useEffect(() => {
    const close = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", close);
    return () => window.removeEventListener("keydown", close);
  }, [onClose]);
  const localSource = localPath ? convertFileSrc(localPath) : undefined;
  const source =
    localSource ??
    (resource.kind === "audio" || resource.kind === "image"
      ? resource.url
      : undefined);
  const title = resourceDisplayTitle(resource);
  return (
    <div
      className="preview-layer fixed inset-0 z-50 flex justify-end bg-black/20 backdrop-blur-[2px]"
      onMouseDown={onClose}
    >
      <aside
        className="preview-drawer animate-detail-in flex h-full w-[min(440px,92vw)] flex-col bg-[#f7f5f0] shadow-[-24px_0_70px_rgba(0,0,0,.16)]"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <header className="flex h-16 shrink-0 items-center border-b border-black/10 px-5">
          <div className="min-w-0">
            <p
              className="text-[10px] font-bold tracking-[.16em]"
              style={{ color: accent }}
            >
              RESOURCE PREVIEW
            </p>
            <h2 className="mt-1 truncate text-sm font-semibold">资源预览</h2>
          </div>
          <button
            onClick={onClose}
            aria-label="关闭预览"
            className="ml-auto grid size-8 place-items-center rounded-lg text-black/40 hover:bg-black/[.06] hover:text-black"
          >
            <X className="size-4" />
          </button>
        </header>
        <div className="min-h-0 flex-1 overflow-y-auto p-5">
          <div className="preview-stage grid min-h-72 place-items-center overflow-hidden rounded-2xl bg-[#1e1f21] text-white">
            {checking ? (
              <LoaderCircle className="size-5 animate-spin text-white/45" />
            ) : resource.kind === "image" && source ? (
              <img
                src={source}
                alt={title}
                className="max-h-[55vh] w-full object-contain"
              />
            ) : resource.kind === "audio" && source ? (
              <div className="w-full px-7 text-center">
                <div className="mx-auto mb-7 grid size-20 place-items-center rounded-full bg-white/10">
                  <Headphones className="size-8 text-white/75" />
                </div>
                <audio key={source} src={source} controls className="w-full" />
              </div>
            ) : resource.kind === "video" && source ? (
              <video
                key={source}
                src={source}
                controls
                className="max-h-[58vh] w-full bg-black"
              />
            ) : resource.kind === "document" && source ? (
              <iframe
                title={title}
                src={source}
                className="h-[58vh] w-full bg-white"
              />
            ) : (
              <div className="max-w-xs px-8 text-center">
                <div className="mx-auto mb-4 grid size-14 place-items-center rounded-full bg-white/10">
                  <Play className="size-5 text-white/65" />
                </div>
                <p className="text-sm font-medium">
                  {resource.kind === "video"
                    ? "下载后可预览视频"
                    : "暂不支持在线预览"}
                </p>
                <p className="mt-2 text-xs leading-5 text-white/45">
                  课程视频会在下载完成并通过校验后使用本地文件预览。
                </p>
              </div>
            )}
          </div>
          <div className="mt-6">
            <div className="flex items-start justify-between gap-4">
              <div className="min-w-0">
                <h3 className="break-words text-base font-semibold leading-6">
                  {title}
                </h3>
                <p className="mt-1 text-[11px] text-black/40">
                  {resourceDisplayExtension(resource)} ·{" "}
                  {kindName(resource.kind)}
                </p>
              </div>
              <span
                className={cn(
                  "shrink-0 rounded-full px-2.5 py-1 text-[10px] font-medium",
                  localPath
                    ? "bg-emerald-100 text-emerald-700"
                    : "bg-black/[.055] text-black/50",
                )}
              >
                {localPath ? "本地文件" : "在线资源"}
              </span>
            </div>
            <dl className="mt-5 divide-y divide-black/[.07] border-y border-black/[.07] text-xs">
              <div className="grid grid-cols-[72px_1fr] py-3">
                <dt className="text-black/35">保存目录</dt>
                <dd className="break-all text-black/65">
                  {resource.subfolder || "根目录"}
                </dd>
              </div>
              <div className="grid grid-cols-[72px_1fr] py-3">
                <dt className="text-black/35">文件大小</dt>
                <dd className="text-black/65">
                  {resource.size ? formatBytes(resource.size) : "下载时获取"}
                </dd>
              </div>
              <div className="grid grid-cols-[72px_1fr] py-3">
                <dt className="text-black/35">当前状态</dt>
                <dd className="text-black/65">{progressStatusText(state)}</dd>
              </div>
            </dl>
          </div>
        </div>
        <footer className="shrink-0 border-t border-black/10 p-5">
          <Button
            variant="accent"
            className="w-full"
            onClick={onDownload}
            disabled={
              state?.status === "downloading" || state?.status === "queued"
            }
          >
            <Download className="size-4" />
            {localPath
              ? "重新下载"
              : state?.status === "downloading"
                ? "正在下载"
                : "下载并处理"}
          </Button>
        </footer>
      </aside>
    </div>
  );
}
