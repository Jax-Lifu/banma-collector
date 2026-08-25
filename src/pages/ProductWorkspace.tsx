import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { useNavigate } from "react-router-dom";
import {
  ArrowLeft,
  BookOpen,
  Check,
  CheckSquare,
  ChevronRight,
  CircleStop,
  Download,
  FileText,
  Folder,
  Headphones,
  Layers3,
  LoaderCircle,
  LogIn,
  LogOut,
  Play,
  RefreshCw,
  RotateCcw,
  UserRound,
} from "lucide-react";
import {
  AlbumProgressPanel,
  MediaRow,
  ResourcePreview,
  buildAlbumProgress,
} from "../components/ResourceViews";
import { Button } from "../components/ui/button";
import { Progress } from "../components/ui/progress";
import type {
  ContentEntry,
  LoginSession,
  ProductContent,
  ProgressState,
  ResourceItem,
  ZebraProduct,
} from "../lib/schema";
import {
  cancelDownload,
  chooseDirectory,
  downloadResources,
  loadAlbumsResources,
  loadContentDetail,
  loadProductCatalog,
  logout,
  revealPath,
} from "../lib/tauri";
import {
  filterResourcesForSelection,
  groupLogicalResources,
  isLoginError,
  readError,
  resourceDownloadPriority,
  videoVariantLabel,
} from "../lib/resources";
import { cn, formatBytes } from "../lib/utils";
import { useCollector } from "../store/collector";

type WorkspaceProductDefinition = {
  id: ZebraProduct;
  name: string;
  eyebrow: string;
  accent: string;
  icon: ReactNode;
};

export function ProductWorkspace({
  product,
  products,
  definition,
  session,
  onLogin,
  onSession,
}: {
  product: ZebraProduct;
  products: WorkspaceProductDefinition[];
  definition: WorkspaceProductDefinition;
  session: LoginSession;
  onLogin: () => void;
  onSession: (session: LoginSession) => void;
}) {
  const navigate = useNavigate();
  const store = useCollector();
  const [content, setContent] = useState<ProductContent>();
  const [trail, setTrail] = useState<
    Array<{ entry: ContentEntry; content: ProductContent }>
  >([]);
  const [notice, setNotice] = useState<string>();
  const [concurrency, setConcurrency] = useState(4);
  const [separateLanguages, setSeparateLanguages] = useState(true);
  const [selectedQualities, setSelectedQualities] = useState<string[]>([
    "1080P",
  ]);
  const [selectedLanguages, setSelectedLanguages] = useState<string[]>([
    "中文",
  ]);
  const [selectedAlbums, setSelectedAlbums] = useState<string[]>([]);
  const [kindFilter, setKindFilter] = useState<string>("all");
  const [cancelling, setCancelling] = useState(false);
  const batchCancelRef = useRef(false);
  const [previewResource, setPreviewResource] = useState<ResourceItem>();
  const closePreview = useCallback(() => setPreviewResource(undefined), []);

  const catalog = useQuery({
    queryKey: ["catalog", product, session.userId],
    queryFn: () => loadProductCatalog(product),
    enabled: session.loggedIn,
    retry: false,
  });
  useEffect(() => {
    if (catalog.data) {
      setContent(catalog.data);
      setTrail([]);
      setSelectedAlbums([]);
      store.setResources(catalog.data.videos);
      store.selectAll(
        filterResourcesForSelection(
          catalog.data.videos,
          selectedQualities,
          selectedLanguages,
        ).map((item) => item.id),
      );
      setNotice(catalog.data.warning ?? undefined);
    }
  }, [catalog.data, product]);

  useEffect(() => {
    if (catalog.error) {
      if (isLoginError(catalog.error)) onLogin();
      setNotice(readError(catalog.error));
    }
  }, [catalog.error]);

  useEffect(() => {
    let stop: (() => void) | undefined;
    listen<ProgressState & { id: string }>("download-progress", ({ payload }) =>
      store.updateProgress(payload.id, payload),
    )
      .then((value) => {
        stop = value;
      })
      .catch(() => undefined);
    return () => stop?.();
  }, []);

  const detail = useMutation({
    mutationFn: (entry: ContentEntry) => loadContentDetail(product, entry),
    onSuccess: (next, entry) => {
      setContent(next);
      setTrail((value) => [...value, { entry, content: next }]);
      setSelectedAlbums([]);
      store.setResources(next.videos);
      store.selectAll(
        filterResourcesForSelection(
          next.videos,
          selectedQualities,
          selectedLanguages,
        ).map((item) => item.id),
      );
      setKindFilter("all");
    },
    onError: (error) => {
      if (isLoginError(error)) onLogin();
      setNotice(readError(error));
    },
  });

  const download = useMutation({
    mutationFn: async () => {
      batchCancelRef.current = false;
      const selected = store.resources.filter((item) =>
        store.selected.includes(item.id),
      );
      selected.forEach((item) =>
        store.updateProgress(item.id, { status: "queued", received: 0 }),
      );
      await downloadResources(
        selected,
        store.outputDir,
        concurrency,
        product,
        separateLanguages,
      );
      if (batchCancelRef.current) throw new Error("下载任务已停止");
    },
    onSuccess: () => setNotice("所选资源下载任务处理完毕"),
    onError: (error) => setNotice(readError(error)),
  });

  const singleDownload = useMutation({
    mutationFn: async (item: ResourceItem) => {
      store.updateProgress(item.id, { status: "queued", received: 0 });
      await downloadResources(
        [item],
        store.outputDir,
        1,
        product,
        separateLanguages,
      );
    },
    onError: (error) => setNotice(readError(error)),
  });

  const retryAllFailed = useMutation({
    mutationFn: async () => {
      batchCancelRef.current = false;
      const failed = store.resources.filter((item) => {
        const s = store.progress[item.id]?.status;
        return s === "failed" || s === "cancelled";
      });
      if (!failed.length) return;
      failed.forEach((item) =>
        store.updateProgress(item.id, { status: "queued", received: 0 }),
      );
      setNotice(`正在重新下载 ${failed.length} 个失败/已取消任务...`);
      await downloadResources(
        failed,
        store.outputDir,
        concurrency,
        product,
        separateLanguages,
      );
      if (batchCancelRef.current) throw new Error("下载任务已停止");
    },
    onSuccess: () => setNotice("重试任务已全部下载完成！"),
    onError: (error) => setNotice(readError(error)),
  });

  const cancelAll = async () => {
    try {
      setCancelling(true);
      batchCancelRef.current = true;
      await cancelDownload();
      setNotice("停止指令已生效，正在终止网络流、媒体进程和排队任务...");
    } catch (err) {
      setCancelling(false);
      setNotice(readError(err));
    }
  };

  const cancelSingle = async (id: string, e?: React.MouseEvent) => {
    e?.stopPropagation();
    try {
      await cancelDownload(id);
    } catch (err) {
      setNotice(readError(err));
    }
  };

  const batchDownloadAlbums = useMutation({
    mutationFn: async ({
      albumIds,
      downloadAfterParse,
    }: {
      albumIds: string[];
      downloadAfterParse: boolean;
    }) => {
      const albums = expandableEntries.filter((entry) =>
        albumIds.includes(entrySelectionKey(entry)),
      );
      batchCancelRef.current = false;
      setNotice(`正在并发解析所选 ${albums.length} 个专辑的全部子项资源...`);
      const resourcesByAlbum: ResourceItem[][] = Array.from({
        length: albums.length,
      });
      const parseConcurrency = Math.min(6, Math.max(2, concurrency));
      let nextIndex = 0;
      let completedAlbums = 0;

      const worker = async () => {
        while (true) {
          if (batchCancelRef.current)
            throw new Error("CANCELLED:已停止解析专辑");
          const index = nextIndex++;
          if (index >= albums.length) return;
          resourcesByAlbum[index] = await loadAlbumsResources(product, [
            albums[index],
          ]);
          completedAlbums += 1;
          const discovered = resourcesByAlbum.reduce(
            (total, items) => total + (items?.length ?? 0),
            0,
          );
          setNotice(
            `正在并发解析专辑 ${completedAlbums}/${albums.length}，已发现 ${discovered} 个资源...`,
          );
        }
      };

      await Promise.all(
        Array.from(
          { length: Math.min(parseConcurrency, albums.length) },
          worker,
        ),
      );
      if (batchCancelRef.current) throw new Error("CANCELLED:已停止解析专辑");
      const resources = resourcesByAlbum.flat();
      if (!resources.length)
        throw new Error("所选专辑未识别到可下载的音频或视频资源");
      const targetResources = resources;
      const selectedResources = filterResourcesForSelection(
        targetResources,
        selectedQualities,
        selectedLanguages,
      );
      setContent((prev) =>
        prev
          ? { ...prev, videos: targetResources }
          : { entries: [], videos: targetResources },
      );
      store.setResources(targetResources);
      store.selectAll(selectedResources.map((item) => item.id));
      if (!downloadAfterParse) {
        setNotice(
          `解析完成：共发现 ${targetResources.length} 个资源，已按清晰度和语言选中 ${selectedResources.length} 个；尚未开始下载。`,
        );
        return;
      }
      selectedResources.forEach((item) =>
        store.updateProgress(item.id, { status: "queued", received: 0 }),
      );
      setNotice(
        `已解析出 ${targetResources.length} 个资源，将按所选清晰度和语言下载 ${selectedResources.length} 个...`,
      );

      const albumErrors: string[] = [];
      for (let index = 0; index < albums.length; index += 1) {
        if (batchCancelRef.current) throw new Error("下载任务已停止");
        const albumResources = filterResourcesForSelection(
          resourcesByAlbum[index] ?? [],
          selectedQualities,
          selectedLanguages,
        ).sort(
          (left, right) =>
            resourceDownloadPriority(left) - resourceDownloadPriority(right),
        );
        if (!albumResources.length) continue;

        const videoCount = albumResources.filter(
          (item) => item.kind === "video",
        ).length;
        setNotice(
          `正在下载专辑 ${index + 1}/${albums.length}：${albums[index].title}，` +
            `共 ${albumResources.length} 个资源（视频优先 ${videoCount} 个）...`,
        );
        try {
          // 专辑之间严格串行；当前专辑内部仍按用户设置的并发数下载。
          await downloadResources(
            albumResources,
            store.outputDir,
            concurrency,
            product,
            separateLanguages,
          );
        } catch (error) {
          if (batchCancelRef.current) throw new Error("下载任务已停止");
          albumErrors.push(`${albums[index].title}：${readError(error)}`);
        }
      }
      if (albumErrors.length) {
        throw new Error(
          `${albumErrors.length} 个专辑存在失败任务：${albumErrors.slice(0, 3).join("；")}`,
        );
      }
    },
    onSuccess: (_, variables) => {
      setSelectedAlbums([]);
      if (variables.downloadAfterParse) {
        setNotice("所选专辑的所有资源已按专辑文件夹下载完成！");
      }
    },
    onError: (error) => setNotice(readError(error).replace(/^CANCELLED:/, "")),
  });

  const isDownloading =
    download.isPending ||
    batchDownloadAlbums.isPending ||
    singleDownload.isPending ||
    retryAllFailed.isPending;
  const isParsingOnly =
    batchDownloadAlbums.isPending &&
    batchDownloadAlbums.variables?.downloadAfterParse === false;
  useEffect(() => {
    if (!isDownloading) setCancelling(false);
  }, [isDownloading]);
  const completed = Object.values(store.progress).filter(
    (item) => item.status === "completed",
  ).length;
  const activeReceived = Object.values(store.progress)
    .filter((item) => item.status === "downloading")
    .reduce((total, item) => total + item.received, 0);
  const failedItems = useMemo(() => {
    return store.resources.filter(
      (item) => store.progress[item.id]?.status === "failed",
    );
  }, [store.resources, store.progress]);
  const cancelledItems = useMemo(() => {
    return store.resources.filter(
      (item) => store.progress[item.id]?.status === "cancelled",
    );
  }, [store.resources, store.progress]);
  const totalFailedOrCancelled = failedItems.length + cancelledItems.length;
  const selectedLogicalCount = useMemo(
    () =>
      groupLogicalResources(
        store.resources.filter((item) => store.selected.includes(item.id)),
        selectedQualities,
        selectedLanguages,
      ).length,
    [store.resources, store.selected, selectedQualities, selectedLanguages],
  );

  const progressTotal = Object.keys(store.progress).length;
  const progress = progressTotal ? (completed / progressTotal) * 100 : 0;
  const loading = catalog.isLoading || detail.isPending;

  function goRoot() {
    setTrail([]);
    setSelectedAlbums([]);
    setContent(catalog.data);
    store.setResources(catalog.data?.videos ?? []);
    store.selectAll(
      filterResourcesForSelection(
        catalog.data?.videos ?? [],
        selectedQualities,
        selectedLanguages,
      ).map((item) => item.id),
    );
    setKindFilter("all");
  }

  function goToTrail(index: number) {
    const level = trail[index];
    if (!level) return;
    setContent(level.content);
    setTrail((value) => value.slice(0, index + 1));
    setSelectedAlbums([]);
    store.setResources(level.content.videos);
    store.selectAll(
      filterResourcesForSelection(
        level.content.videos,
        selectedQualities,
        selectedLanguages,
      ).map((item) => item.id),
    );
    setKindFilter("all");
  }

  const filteredResources = useMemo(() => {
    if (!content?.videos) return [];
    if (kindFilter === "all") return content.videos;
    return content.videos.filter((item) => item.kind === kindFilter);
  }, [content?.videos, kindFilter]);

  const displayedResourceGroups = useMemo(
    () =>
      groupLogicalResources(
        filteredResources,
        selectedQualities,
        selectedLanguages,
      ),
    [filteredResources, selectedQualities, selectedLanguages],
  );

  const resourceCounts = useMemo(() => {
    const logical = groupLogicalResources(
      content?.videos ?? [],
      selectedQualities,
      selectedLanguages,
    );
    const counts: Record<string, number> = { all: logical.length };
    for (const group of logical) {
      counts[group.resource.kind] = (counts[group.resource.kind] ?? 0) + 1;
    }
    return counts;
  }, [content?.videos, selectedQualities, selectedLanguages]);

  const expandableEntries = useMemo(() => {
    return (content?.entries ?? []).filter(
      (e) => e.hasDetail !== false && !e.locked,
    );
  }, [content?.entries]);

  const albumProgress = useMemo(
    () => buildAlbumProgress(store.resources, store.progress),
    [store.resources, store.progress],
  );
  const preparingAlbums =
    batchDownloadAlbums.isPending && albumProgress.length === 0
      ? expandableEntries
          .filter((entry) => selectedAlbums.includes(entrySelectionKey(entry)))
          .map((entry) => entry.title)
      : [];

  function toggleAlbumSelect(id: string, e: React.MouseEvent) {
    e.stopPropagation();
    if (!selectedAlbums.includes(id)) store.clearSelection();
    setSelectedAlbums((prev) =>
      prev.includes(id) ? prev.filter((item) => item !== id) : [...prev, id],
    );
  }

  function selectAllAlbums() {
    store.clearSelection();
    setSelectedAlbums(expandableEntries.map(entrySelectionKey));
  }

  function clearSelectedAlbums() {
    setSelectedAlbums([]);
    store.clearSelection();
  }

  function toggleQuality(quality: string) {
    const next =
      quality === "全部"
        ? ["全部"]
        : selectedQualities.includes("全部")
          ? [quality]
          : selectedQualities.includes(quality)
            ? selectedQualities.length === 1
              ? selectedQualities
              : selectedQualities.filter((item) => item !== quality)
            : [...selectedQualities, quality];
    setSelectedQualities(next);
    store.selectAll(
      filterResourcesForSelection(store.resources, next, selectedLanguages).map(
        (item) => item.id,
      ),
    );
  }

  function toggleLanguage(language: string) {
    const next =
      language === "全部"
        ? ["全部"]
        : selectedLanguages.includes("全部")
          ? [language]
          : selectedLanguages.includes(language)
            ? selectedLanguages.length === 1
              ? selectedLanguages
              : selectedLanguages.filter((item) => item !== language)
            : [...selectedLanguages, language];
    setSelectedLanguages(next);
    store.selectAll(
      filterResourcesForSelection(store.resources, selectedQualities, next).map(
        (item) => item.id,
      ),
    );
  }

  const signOut = useMutation({
    mutationFn: () => logout(product),
    onSuccess: (value) => {
      onSession(value);
      setContent(undefined);
      store.setResources([]);
      onLogin();
    },
  });

  return (
    <div className="workspace-shell h-full">
      <aside className="workspace-nav flex flex-col border-r border-black/10 bg-[#efede7] p-3">
        <div className="px-2 py-3">
          <Brand />
        </div>
        <button
          onClick={() => navigate("/")}
          className="nav-action mb-5 flex h-9 items-center rounded-md px-2 text-xs text-black/50 hover:bg-white/60"
        >
          <ArrowLeft className="mr-2 size-4 shrink-0" />
          <span>切换产品</span>
        </button>
        <p className="nav-label px-2 pb-2 text-[10px] font-bold tracking-[.16em] text-black/30">
          产品
        </p>
        {products.map((item) => (
          <button
            key={item.id}
            title={item.name}
            onClick={() => navigate(`/${item.id}`)}
            className={cn(
              "product-nav-item mb-1 flex h-10 items-center rounded-lg px-2.5 text-sm",
              item.id === product
                ? "bg-white font-semibold shadow-sm"
                : "text-black/50 hover:bg-white/50",
            )}
          >
            <span
              className="product-nav-icon mr-2.5 grid size-6 shrink-0 place-items-center rounded-md text-white [&>svg]:size-3.5"
              style={{ background: item.accent }}
            >
              {item.icon}
            </span>
            <span className="nav-copy">{item.name}</span>
          </button>
        ))}
        <div className="account-block mt-auto rounded-lg border border-black/8 bg-white/55 p-3">
          <p className="nav-copy truncate text-xs font-semibold">
            {session.nickname || session.phoneMasked || "当前账号"}
          </p>
          <p className="nav-copy mt-1 text-[10px] text-black/40">
            {definition.name} · 独立会话
          </p>
          <button
            title="退出登录"
            onClick={() => signOut.mutate()}
            className="mt-3 flex items-center text-[11px] text-black/45 hover:text-black"
          >
            <LogOut className="mr-1.5 size-3" />
            <span className="nav-copy">退出登录</span>
          </button>
        </div>
      </aside>

      <main className="workspace-main min-w-0 overflow-y-auto bg-[#fbfaf7]">
        <header className="workspace-header sticky top-0 z-10 flex h-16 items-center border-b border-black/10 bg-[#fbfaf7]/95 px-7 backdrop-blur">
          <div>
            <p
              className="text-[10px] font-bold tracking-[.18em]"
              style={{ color: definition.accent }}
            >
              {definition.eyebrow} / LIBRARY
            </p>
            <h1 className="mt-1 text-lg font-semibold">
              {definition.name}资源库
            </h1>
          </div>
          <Button
            variant="ghost"
            size="sm"
            className="ml-auto"
            onClick={() => catalog.refetch()}
          >
            <RefreshCw
              className={cn("size-3.5", catalog.isFetching && "animate-spin")}
            />
            刷新
          </Button>
        </header>

        <section className="workspace-content p-7">
          <div className="mb-6 flex items-center gap-1 text-xs text-black/40">
            <button onClick={goRoot} className="hover:text-black font-medium">
              全部内容
            </button>
            {trail.map(({ entry }, index) => (
              <span
                key={`${entry.kind}-${entry.id}`}
                className="flex min-w-0 items-center"
              >
                <ChevronRight className="mx-1 size-3" />
                <button
                  type="button"
                  onClick={() => goToTrail(index)}
                  disabled={index === trail.length - 1}
                  className={cn(
                    "max-w-40 truncate font-medium",
                    index === trail.length - 1
                      ? "cursor-default text-black/65"
                      : "text-black/45 hover:text-black hover:underline",
                  )}
                >
                  {entry.title}
                </button>
              </span>
            ))}
          </div>

          {!session.loggedIn ? (
            <LoginRequired onLogin={onLogin} />
          ) : loading ? (
            <Loading message="正在读取账号内容…" />
          ) : (
            <>
              {(preparingAlbums.length > 0 || albumProgress.length > 0) && (
                <AlbumProgressPanel
                  groups={albumProgress}
                  preparing={preparingAlbums}
                  accent={definition.accent}
                />
              )}
              <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
                <SectionTitle
                  title={
                    product === "zebra"
                      ? "拓展内容、VIP课程与音频专辑"
                      : "课程与学习任务"
                  }
                  count={content?.entries.length ?? 0}
                />
                {trail.length === 0 && expandableEntries.length > 0 && (
                  <div className="flex items-center gap-1.5">
                    <span className="mr-1 text-[11px] text-black/40">
                      {selectedAlbums.length > 0
                        ? `已选 ${selectedAlbums.length} 个专辑`
                        : `共 ${expandableEntries.length} 个可下载专辑`}
                    </span>
                    <Button
                      variant="ghost"
                      size="sm"
                      className="h-7 px-2 text-xs text-black/50 hover:text-black"
                      onClick={selectAllAlbums}
                    >
                      <CheckSquare className="mr-1 size-3.5" />
                      全选专辑
                    </Button>
                    {selectedAlbums.length > 0 && (
                      <Button
                        variant="ghost"
                        size="sm"
                        className="h-7 px-2 text-xs text-black/40 hover:text-black"
                        onClick={clearSelectedAlbums}
                      >
                        清空全部
                      </Button>
                    )}
                  </div>
                )}
              </div>

              <div className="album-grid mb-8 grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
                {content?.entries.map((entry) => {
                  const selectionKey = entrySelectionKey(entry);
                  const isSelected = selectedAlbums.includes(selectionKey);
                  const canExpand = entry.hasDetail !== false && !entry.locked;
                  return (
                    <div
                      key={`${entry.kind}-${entry.id}`}
                      onClick={() => canExpand && detail.mutate(entry)}
                      className={cn(
                        "group relative flex items-center rounded-xl border p-3 text-left transition-all",
                        canExpand
                          ? "cursor-pointer hover:border-black/25 hover:shadow-sm"
                          : "opacity-60 cursor-not-allowed",
                        isSelected
                          ? "border-amber-400 bg-amber-50/40"
                          : "border-black/8 bg-white",
                      )}
                    >
                      {trail.length === 0 && canExpand && (
                        <button
                          onClick={(e) => toggleAlbumSelect(selectionKey, e)}
                          className={cn(
                            "mr-2.5 grid size-5 shrink-0 place-items-center rounded border transition-colors",
                            isSelected
                              ? "bg-amber-500 border-amber-500 text-white"
                              : "border-black/20 hover:border-black/40 bg-white",
                          )}
                          title={isSelected ? "取消选择" : "选择此专辑批量下载"}
                        >
                          {isSelected ? <Check className="size-3.5" /> : null}
                        </button>
                      )}
                      {entry.coverUrl ? (
                        <img
                          src={entry.coverUrl}
                          alt=""
                          className="mr-3 size-12 shrink-0 rounded-lg object-cover bg-black/5"
                        />
                      ) : (
                        <div className="mr-3 grid size-12 shrink-0 place-items-center rounded-lg bg-black/5 text-black/30">
                          <BookOpen className="size-5" />
                        </div>
                      )}
                      <div className="min-w-0 flex-1">
                        <p className="truncate text-xs font-semibold text-[#1b1a18] group-hover:text-black">
                          {entry.title}
                        </p>
                        <p className="mt-0.5 truncate text-[11px] text-black/40">
                          {entry.subtitle || kindLabel(entry.kind)}
                        </p>
                      </div>
                      {canExpand && (
                        <ChevronRight className="ml-2 size-4 shrink-0 text-black/20 transition-transform group-hover:translate-x-0.5 group-hover:text-black/40" />
                      )}
                    </div>
                  );
                })}
              </div>

              <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
                <div className="flex flex-wrap items-center gap-2">
                  <SectionTitle
                    title="可下载资源"
                    count={displayedResourceGroups.length}
                  />
                  {content?.videos && content.videos.length > 0 && (
                    <div className="flex items-center gap-1 rounded-lg border border-black/8 bg-black/[.03] p-0.5 text-xs">
                      <button
                        onClick={() => setKindFilter("all")}
                        className={cn(
                          "rounded-md px-2 py-0.5 text-[11px] transition-colors",
                          kindFilter === "all"
                            ? "bg-white font-medium shadow-sm text-black"
                            : "text-black/50 hover:text-black",
                        )}
                      >
                        全部 ({resourceCounts.all || 0})
                      </button>
                      {resourceCounts.audio ? (
                        <button
                          onClick={() => setKindFilter("audio")}
                          className={cn(
                            "flex items-center gap-1 rounded-md px-2 py-0.5 text-[11px] transition-colors",
                            kindFilter === "audio"
                              ? "bg-white font-medium shadow-sm text-purple-700"
                              : "text-black/50 hover:text-black",
                          )}
                        >
                          <Headphones className="size-3" />
                          音频 ({resourceCounts.audio})
                        </button>
                      ) : null}
                      {resourceCounts.video ? (
                        <button
                          onClick={() => setKindFilter("video")}
                          className={cn(
                            "flex items-center gap-1 rounded-md px-2 py-0.5 text-[11px] transition-colors",
                            kindFilter === "video"
                              ? "bg-white font-medium shadow-sm text-blue-700"
                              : "text-black/50 hover:text-black",
                          )}
                        >
                          <Play className="size-3" />
                          视频 ({resourceCounts.video})
                        </button>
                      ) : null}
                      {resourceCounts.document ? (
                        <button
                          onClick={() => setKindFilter("document")}
                          className={cn(
                            "flex items-center gap-1 rounded-md px-2 py-0.5 text-[11px] transition-colors",
                            kindFilter === "document"
                              ? "bg-white font-medium shadow-sm text-amber-700"
                              : "text-black/50 hover:text-black",
                          )}
                        >
                          <FileText className="size-3" />
                          文档 ({resourceCounts.document})
                        </button>
                      ) : null}
                    </div>
                  )}
                </div>
                {displayedResourceGroups.length > 0 && (
                  <div className="flex items-center gap-2">
                    <Button
                      variant="ghost"
                      size="sm"
                      className="h-7 px-2 text-xs text-black/50 hover:text-black"
                      onClick={() =>
                        store.selectAll(
                          filterResourcesForSelection(
                            filteredResources,
                            selectedQualities,
                            selectedLanguages,
                          ).map((item) => item.id),
                        )
                      }
                    >
                      <CheckSquare className="mr-1 size-3.5" />
                      全选本类
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      className="h-7 px-2 text-xs text-black/40 hover:text-black"
                      onClick={() => {
                        store.clearSelection();
                        setSelectedAlbums([]);
                      }}
                    >
                      清空
                    </Button>
                  </div>
                )}
              </div>

              <div className="overflow-hidden rounded-xl border border-black/10 bg-white">
                {displayedResourceGroups.length ? (
                  displayedResourceGroups.map(
                    ({ resource: video, variants }) => {
                      const selectableVariants = filterResourcesForSelection(
                        variants,
                        selectedQualities,
                        selectedLanguages,
                      );
                      const allSelected =
                        selectableVariants.length > 0 &&
                        selectableVariants.every((item) =>
                          store.selected.includes(item.id),
                        );
                      const variantLabel =
                        video.kind === "video"
                          ? videoVariantLabel(variants)
                          : undefined;
                      return (
                        <MediaRow
                          key={video.id}
                          resource={video}
                          selected={allSelected}
                          state={store.progress[video.id]}
                          variantLabel={variantLabel}
                          onToggle={() => {
                            const next = new Set(store.selected);
                            for (const item of selectableVariants) {
                              if (allSelected) next.delete(item.id);
                              else next.add(item.id);
                            }
                            store.selectAll([...next]);
                          }}
                          onCancel={(e) => cancelSingle(video.id, e)}
                          onRetry={(e) => {
                            e.stopPropagation();
                            singleDownload.mutate(video);
                          }}
                          onPreview={() => setPreviewResource(video)}
                          accent={definition.accent}
                        />
                      );
                    },
                  )
                ) : (
                  <EmptyContent />
                )}
              </div>
            </>
          )}
        </section>
      </main>

      <aside className="workspace-queue flex flex-col border-l border-black/10 bg-[#f3f1eb]">
        <div className="border-b border-black/10 px-5 py-5">
          <p className="text-sm font-semibold">下载队列</p>
          <p className="mt-1 text-[11px] text-black/40">
            {trail.length === 0 && selectedAlbums.length > 0
              ? `已勾选 ${selectedAlbums.length} 个专辑（将自动按专辑分类建立文件夹下载）`
              : `已选择 ${selectedLogicalCount} 个课程资源`}
          </p>
        </div>

        <div className="flex-1 overflow-y-auto p-5">
          <div className="rounded-xl bg-white p-4 shadow-sm">
            <p className="text-[10px] font-bold tracking-[.14em] text-black/35">
              保存位置
            </p>
            <button
              onClick={async () => {
                const path = await chooseDirectory();
                if (typeof path === "string") store.setOutputDir(path);
              }}
              className="mt-3 flex w-full items-center text-left text-xs"
            >
              <Folder className="mr-2 size-4 shrink-0 text-black/35" />
              <span className="min-w-0 flex-1 truncate">{store.outputDir}</span>
              <ChevronRight className="size-4 text-black/25" />
            </button>
          </div>

          <div className="mt-4 flex items-center justify-between text-xs">
            <span className="text-black/50">并发下载</span>
            <select
              value={concurrency}
              onChange={(e) => setConcurrency(Number(e.target.value))}
              className="rounded-md border border-black/10 bg-white px-2 py-1.5 outline-none"
            >
              {[2, 4, 6, 8, 12].map((value) => (
                <option key={value}>{value}</option>
              ))}
            </select>
          </div>

          {product === "pedia" && (
            <div className="mt-5 space-y-4">
              <div>
                <div className="flex items-center justify-between text-xs">
                  <span className="text-black/50">视频清晰度</span>
                  <span className="text-[10px] text-black/35">可多选</span>
                </div>
                <div className="mt-2 flex flex-wrap gap-1.5">
                  {["1080P", "720P", "4K", "标清", "全部"].map((quality) => {
                    const active = selectedQualities.includes(quality);
                    return (
                      <button
                        key={quality}
                        type="button"
                        onClick={() => toggleQuality(quality)}
                        className={cn(
                          "rounded-md border px-2 py-1 text-[10px] font-medium transition-colors",
                          active
                            ? "border-[#f05a38] bg-[#fff0e9] text-[#d94729]"
                            : "border-black/10 bg-white text-black/45 hover:border-black/25",
                        )}
                      >
                        {quality === "全部" ? "全部清晰度" : quality}
                      </button>
                    );
                  })}
                </div>
              </div>
              <div>
                <div className="flex items-center justify-between text-xs">
                  <span className="text-black/50">视频语言</span>
                  <span className="text-[10px] text-black/35">可多选</span>
                </div>
                <div className="mt-2 flex flex-wrap gap-1.5">
                  {["中文", "英文", "全部"].map((language) => {
                    const active = selectedLanguages.includes(language);
                    return (
                      <button
                        key={language}
                        type="button"
                        onClick={() => toggleLanguage(language)}
                        className={cn(
                          "rounded-md border px-2 py-1 text-[10px] font-medium transition-colors",
                          active
                            ? "border-[#f05a38] bg-[#fff0e9] text-[#d94729]"
                            : "border-black/10 bg-white text-black/45 hover:border-black/25",
                        )}
                      >
                        {language === "全部" ? "全部语言" : language}
                      </button>
                    );
                  })}
                </div>
              </div>
              <div>
                <div className="flex items-center justify-between text-xs">
                  <span className="text-black/50">下载目录结构</span>
                  <span className="text-[10px] text-black/35">默认按语言</span>
                </div>
                <div className="mt-2 grid grid-cols-2 gap-1.5">
                  <button
                    type="button"
                    onClick={() => setSeparateLanguages(true)}
                    className={cn(
                      "rounded-md border px-2 py-1.5 text-[10px] font-medium transition-colors",
                      separateLanguages
                        ? "border-[#f05a38] bg-[#fff0e9] text-[#d94729]"
                        : "border-black/10 bg-white text-black/45 hover:border-black/25",
                    )}
                  >
                    按语言分目录
                  </button>
                  <button
                    type="button"
                    onClick={() => setSeparateLanguages(false)}
                    className={cn(
                      "rounded-md border px-2 py-1.5 text-[10px] font-medium transition-colors",
                      !separateLanguages
                        ? "border-[#f05a38] bg-[#fff0e9] text-[#d94729]"
                        : "border-black/10 bg-white text-black/45 hover:border-black/25",
                    )}
                  >
                    按课程混合
                  </button>
                </div>
                <p className="mt-1.5 text-[10px] leading-4 text-black/35">
                  按语言时会在保存位置下建立“中文”和“英文”目录。
                </p>
              </div>
            </div>
          )}

          {(isDownloading || completed > 0 || totalFailedOrCancelled > 0) && (
            <div className="mt-5 space-y-2">
              <div className="flex justify-between text-[11px] text-black/45">
                <span>{isDownloading ? "正在下载" : "下载状态"}</span>
                <span>
                  {completed}/{progressTotal}
                </span>
              </div>
              <Progress value={progress} />
              {isDownloading && activeReceived > 0 && (
                <div className="text-right text-[10px] text-black/40">
                  当前任务已写入 {formatBytes(activeReceived)}
                </div>
              )}
              {totalFailedOrCancelled > 0 && (
                <div className="flex justify-between text-[10px] text-red-500 font-medium">
                  <span>失败/已取消任务</span>
                  <span>{totalFailedOrCancelled} 个</span>
                </div>
              )}
            </div>
          )}

          {notice && (
            <p className="mt-4 rounded-lg bg-black/[.045] p-3 text-xs leading-5 text-black/55">
              {notice}
            </p>
          )}
        </div>

        <div className="border-t border-black/10 p-5 space-y-2.5">
          {isDownloading ? (
            <Button
              variant="outline"
              className="w-full border-red-200 bg-red-50 text-red-600 hover:bg-red-100 hover:text-red-700"
              onClick={cancelAll}
              disabled={cancelling}
            >
              {cancelling ? (
                <LoaderCircle className="size-4 animate-spin" />
              ) : (
                <CircleStop className="size-4" />
              )}
              {isParsingOnly ? "停止解析" : "停止全部下载"}
            </Button>
          ) : (
            <>
              {totalFailedOrCancelled > 0 && (
                <Button
                  variant="outline"
                  className="w-full border-amber-300 bg-amber-50 text-amber-800 hover:bg-amber-100"
                  onClick={() => retryAllFailed.mutate()}
                  disabled={retryAllFailed.isPending}
                >
                  <RotateCcw
                    className={cn(
                      "size-3.5 mr-1.5",
                      retryAllFailed.isPending && "animate-spin",
                    )}
                  />
                  重试全部失败/中断 ({totalFailedOrCancelled})
                </Button>
              )}
              {trail.length === 0 && selectedAlbums.length > 0 && (
                <Button
                  variant="outline"
                  className="w-full"
                  disabled={isDownloading}
                  onClick={() =>
                    batchDownloadAlbums.mutate({
                      albumIds: selectedAlbums,
                      downloadAfterParse: false,
                    })
                  }
                >
                  <Layers3 className="size-4" />
                  仅解析所选专辑 ({selectedAlbums.length})
                </Button>
              )}
              <Button
                variant="accent"
                className="w-full"
                disabled={
                  !(
                    store.selected.length > 0 ||
                    (trail.length === 0 && selectedAlbums.length > 0)
                  ) || isDownloading
                }
                onClick={() => {
                  if (trail.length === 0 && selectedAlbums.length > 0) {
                    batchDownloadAlbums.mutate({
                      albumIds: selectedAlbums,
                      downloadAfterParse: true,
                    });
                  } else if (store.selected.length > 0) {
                    download.mutate();
                  }
                }}
              >
                <Download className="size-4" />
                {trail.length === 0 && selectedAlbums.length > 0
                  ? `下载所选专辑 (${selectedAlbums.length})`
                  : `下载所选内容 (${selectedLogicalCount})`}
              </Button>
            </>
          )}
          <button
            onClick={() => revealPath(store.outputDir)}
            className="w-full text-center text-[11px] text-black/40 hover:text-black"
          >
            打开下载文件夹
          </button>
        </div>
      </aside>
      {previewResource && (
        <ResourcePreview
          resource={previewResource}
          outputDir={store.outputDir}
          separateLanguages={separateLanguages}
          state={store.progress[previewResource.id]}
          accent={definition.accent}
          onClose={closePreview}
          onDownload={() => singleDownload.mutate(previewResource)}
        />
      )}
    </div>
  );
}

function Brand() {
  return (
    <div className="flex items-center">
      <span className="grid size-7 shrink-0 place-items-center rounded-lg bg-[#1b1a18] text-[11px] font-black text-white">
        Z
      </span>
      <span className="nav-copy ml-2.5 text-sm font-semibold tracking-tight">
        斑马资源库
      </span>
    </div>
  );
}
function SectionTitle({ title, count }: { title: string; count: number }) {
  return (
    <div className="flex items-end">
      <h2 className="text-sm font-semibold">{title}</h2>
      <span className="ml-2 text-[10px] text-black/35">{count}</span>
    </div>
  );
}
function entrySelectionKey(entry: ContentEntry) {
  return `${entry.kind}:${entry.id}`;
}

function Loading({ message }: { message?: string }) {
  return (
    <div className="grid min-h-72 place-items-center text-center">
      <div>
        <LoaderCircle className="mx-auto mb-3 size-5 animate-spin text-black/35" />
        <p className="text-xs text-black/45">
          {message || "正在读取账号内容…"}
        </p>
      </div>
    </div>
  );
}
function EmptyContent() {
  return (
    <div className="grid min-h-36 place-items-center p-6 text-center">
      <div>
        <Layers3 className="mx-auto mb-2 size-5 text-black/20" />
        <p className="text-xs text-black/45">
          点击上方专辑卡片进入详情，或勾选专辑后点击「批量下载选中专辑」按分类下载
        </p>
      </div>
    </div>
  );
}
function LoginRequired({ onLogin }: { onLogin: () => void }) {
  return (
    <div className="grid min-h-[420px] place-items-center text-center">
      <div>
        <div className="mx-auto mb-4 grid size-12 place-items-center rounded-full bg-black/[.05]">
          <UserRound className="size-5" />
        </div>
        <h2 className="font-semibold">登录后查看账号内容</h2>
        <p className="mt-2 text-xs text-black/45">每个产品使用独立登录会话。</p>
        <Button className="mt-5" onClick={onLogin}>
          <LogIn className="size-4" />
          手机号登录
        </Button>
      </div>
    </div>
  );
}
function kindLabel(kind: ContentEntry["kind"]) {
  return (
    (
      {
        pack: "课程包",
        course: "课程",
        mission: "学习任务",
        episode: "课节",
        unit: "单元",
        album: "音频专辑",
        song: "单曲",
        audio: "音频",
        book: "绘本",
      } as const
    )[kind] ?? "内容"
  );
}
