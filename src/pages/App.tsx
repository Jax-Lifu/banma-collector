import { useEffect, useState, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useLocation, useNavigate } from "react-router-dom";
import { BookOpen, ChevronRight, GraduationCap, Sparkles } from "lucide-react";
import { LoginDialog } from "../components/LoginDialog";
import { ProductWorkspace } from "./ProductWorkspace";
import type { LoginSession, ZebraProduct } from "../lib/schema";
import { getLoginSession } from "../lib/tauri";
import { cn } from "../lib/utils";

type ProductDefinition = {
  id: ZebraProduct;
  name: string;
  short: string;
  eyebrow: string;
  description: string;
  accent: string;
  pale: string;
  icon: ReactNode;
};

const products: ProductDefinition[] = [
  {
    id: "pedia",
    name: "斑马百科",
    short: "百科",
    eyebrow: "EXPLORE",
    description: "浏览百科课程包，提取课程中的视频与文档资源。",
    accent: "#f05a38",
    pale: "#fff0e9",
    icon: <BookOpen />,
  },
  {
    id: "aioral",
    name: "斑马口语",
    short: "口语",
    eyebrow: "SPEAK",
    description: "读取口语学习任务，归档账号内的音视频与学习内容。",
    accent: "#2879d0",
    pale: "#eaf3ff",
    icon: <GraduationCap />,
  },
  {
    id: "zebra",
    name: "斑马AI学",
    short: "AI学",
    eyebrow: "EXPAND",
    description:
      "读取拓展与VIP课程、随身听与故事专辑，下载账号可访问的音频、视频与绘本资源。",
    accent: "#7552c8",
    pale: "#f1edff",
    icon: <Sparkles />,
  },
];

const emptySession: LoginSession = { loggedIn: false };

export function App() {
  const location = useLocation();
  const navigate = useNavigate();
  const routeProduct = products.find(
    (item) => location.pathname === `/${item.id}`,
  )?.id;
  const [sessions, setSessions] = useState<Record<ZebraProduct, LoginSession>>({
    pedia: emptySession,
    aioral: emptySession,
    zebra: emptySession,
  });
  const [loginOpen, setLoginOpen] = useState(false);
  const [loginProduct, setLoginProduct] = useState<ZebraProduct>(
    routeProduct ?? "pedia",
  );

  useEffect(() => {
    if (
      !(window as Window & { __TAURI_INTERNALS__?: unknown })
        .__TAURI_INTERNALS__
    )
      return;
    const appWindow = getCurrentWindow();
    let stop: (() => void) | undefined;
    const applyWindowSize = async (physicalSize?: {
      width: number;
      height: number;
    }) => {
      try {
        const [size, scale] = await Promise.all([
          physicalSize ? Promise.resolve(physicalSize) : appWindow.innerSize(),
          appWindow.scaleFactor(),
        ]);
        const factor = Number(scale) || window.devicePixelRatio || 1;
        document.documentElement.style.setProperty(
          "--app-window-width",
          `${Math.round(size.width / factor)}px`,
        );
        document.documentElement.style.setProperty(
          "--app-window-height",
          `${Math.round(size.height / factor)}px`,
        );
      } catch {
        // 普通浏览器预览没有原生窗口 API，继续使用 CSS 视口尺寸。
      }
    };
    void applyWindowSize();
    void appWindow
      .onResized(({ payload }) => {
        void applyWindowSize(payload);
      })
      .then((unlisten) => {
        stop = unlisten;
      });
    return () => {
      stop?.();
      document.documentElement.style.removeProperty("--app-window-width");
      document.documentElement.style.removeProperty("--app-window-height");
    };
  }, []);

  const sessionQuery = useQuery({
    queryKey: ["product-sessions"],
    queryFn: async () =>
      Object.fromEntries(
        await Promise.all(
          products.map(async ({ id }) => [id, await getLoginSession(id)]),
        ),
      ) as Record<ZebraProduct, LoginSession>,
    retry: false,
  });
  useEffect(() => {
    if (sessionQuery.data) setSessions(sessionQuery.data);
  }, [sessionQuery.data]);

  function enter(product: ZebraProduct) {
    if (!sessions[product].loggedIn) {
      setLoginProduct(product);
      setLoginOpen(true);
      return;
    }
    navigate(`/${product}`);
  }

  function onLoggedIn(session: LoginSession) {
    const product = session.product ?? loginProduct;
    setSessions((value) => ({ ...value, [product]: session }));
    navigate(`/${product}`);
  }

  return (
    <div className="h-full overflow-hidden bg-[#f7f5f0] text-[#1b1a18]">
      {routeProduct ? (
        <ProductWorkspace
          product={routeProduct}
          products={products}
          definition={products.find((item) => item.id === routeProduct)!}
          session={sessions[routeProduct]}
          onLogin={() => {
            setLoginProduct(routeProduct);
            setLoginOpen(true);
          }}
          onSession={(value) =>
            setSessions((all) => ({ ...all, [routeProduct]: value }))
          }
        />
      ) : (
        <ProductGate sessions={sessions} onEnter={enter} />
      )}
      <LoginDialog
        open={loginOpen}
        initialProduct={loginProduct}
        onClose={() => setLoginOpen(false)}
        onLoggedIn={onLoggedIn}
      />
    </div>
  );
}

function ProductGate({
  sessions,
  onEnter,
}: {
  sessions: Record<ZebraProduct, LoginSession>;
  onEnter: (product: ZebraProduct) => void;
}) {
  return (
    <div className="flex h-full flex-col">
      <header className="drag-region flex h-16 shrink-0 items-center border-b border-black/10 px-7">
        <Brand />
        <div className="ml-auto text-xs text-black/40">选择一个产品开始</div>
      </header>
      <main className="product-gate-grid grid min-h-0 flex-1 grid-cols-3">
        {products.map((product, index) => (
          <button
            key={product.id}
            onClick={() => onEnter(product.id)}
            className={cn(
              "group relative flex min-w-0 flex-col overflow-hidden border-black/10 p-9 text-left transition-colors",
              index > 0 && "border-l",
            )}
            style={{ background: product.pale }}
          >
            <div className="flex items-center justify-between">
              <span
                className="text-[10px] font-bold tracking-[.22em]"
                style={{ color: product.accent }}
              >
                {product.eyebrow}
              </span>
              <span
                className={cn(
                  "rounded-full px-2.5 py-1 text-[10px]",
                  sessions[product.id].loggedIn
                    ? "bg-emerald-600 text-white"
                    : "bg-black/5 text-black/45",
                )}
              >
                {sessions[product.id].loggedIn ? "已登录" : "需要登录"}
              </span>
            </div>
            <div className="my-auto">
              <div
                className="mb-7 grid size-14 place-items-center rounded-2xl text-white shadow-lg [&>svg]:size-6"
                style={{ background: product.accent }}
              >
                {product.icon}
              </div>
              <h1 className="text-[clamp(30px,3vw,48px)] font-semibold tracking-[-.055em]">
                {product.name}
              </h1>
              <p className="mt-4 max-w-xs text-sm leading-7 text-black/50">
                {product.description}
              </p>
            </div>
            <div className="flex items-center border-t border-black/10 pt-6 text-sm font-semibold">
              {sessions[product.id].loggedIn ? "进入资源库" : "手机号登录"}
              <ChevronRight className="ml-auto size-5 transition-transform group-hover:translate-x-1" />
            </div>
          </button>
        ))}
      </main>
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
