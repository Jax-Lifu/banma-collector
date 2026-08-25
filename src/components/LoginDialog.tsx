import { useEffect, useState } from "react";
import { useMutation } from "@tanstack/react-query";
import {
  KeyRound,
  LoaderCircle,
  LockKeyhole,
  MessageSquareText,
  ShieldCheck,
  X,
} from "lucide-react";
import { z } from "zod";
import { phoneLogin, requestSms } from "../lib/tauri";
import type { LoginSession, ZebraProduct } from "../lib/schema";
import { cn } from "../lib/utils";
import { Button } from "./ui/button";

const phoneSchema = z
  .string()
  .regex(/^1[3-9]\d{9}$/, "请输入有效的中国大陆手机号");
const codeSchema = z.string().regex(/^\d{4,8}$/, "请输入短信中的数字验证码");

export function LoginDialog({
  open,
  initialProduct = "pedia",
  onClose,
  onLoggedIn,
}: {
  open: boolean;
  initialProduct?: ZebraProduct;
  onClose: () => void;
  onLoggedIn: (session: LoginSession) => void;
}) {
  const [product, setProduct] = useState<ZebraProduct>(initialProduct);
  const [phone, setPhone] = useState("");
  const [code, setCode] = useState("");
  const [countdown, setCountdown] = useState(0);
  const [error, setError] = useState<string>();

  useEffect(() => {
    if (!open) return;
    setProduct(initialProduct);
    const onKey = (event: KeyboardEvent) => event.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, initialProduct, onClose]);

  useEffect(() => {
    if (countdown <= 0) return;
    const timer = window.setInterval(
      () => setCountdown((value) => Math.max(0, value - 1)),
      1000,
    );
    return () => window.clearInterval(timer);
  }, [countdown]);

  const sms = useMutation({
    mutationFn: async () => {
      const validPhone = phoneSchema.parse(phone.trim());
      await requestSms(validPhone, product);
    },
    onSuccess: () => {
      setCountdown(60);
      setError(undefined);
    },
    onError: (value) => setError(readError(value)),
  });
  const login = useMutation({
    mutationFn: () =>
      phoneLogin(
        phoneSchema.parse(phone.trim()),
        codeSchema.parse(code.trim()),
        product,
      ),
    onSuccess: (session) => {
      setError(undefined);
      onLoggedIn(session);
      onClose();
    },
    onError: (value) => setError(readError(value)),
  });

  if (!open) return null;
  return (
    <div
      className="fixed inset-0 z-50 grid place-items-center bg-black/35 p-6 backdrop-blur-[2px]"
      onMouseDown={(e) => e.target === e.currentTarget && onClose()}
    >
      <section
        role="dialog"
        aria-modal="true"
        aria-labelledby="login-title"
        className="w-full max-w-[430px] overflow-hidden rounded-xl border border-black/10 bg-[#fbfaf7] shadow-[0_24px_80px_rgba(0,0,0,.24)]"
      >
        <div className="flex items-start border-b border-black/8 px-6 pb-5 pt-6">
          <div className="mr-3 grid size-10 shrink-0 place-items-center rounded-lg bg-[#191918] text-white">
            <LockKeyhole className="size-4.5" />
          </div>
          <div>
            <h2
              id="login-title"
              className="text-lg font-semibold tracking-tight"
            >
              手机号登录
            </h2>
            <p className="mt-1 text-xs leading-5 text-black/45">
              登录后可访问账号下已购内容与受保护资源。
            </p>
          </div>
          <Button
            variant="ghost"
            size="icon"
            className="ml-auto -mr-2 -mt-2"
            onClick={onClose}
            aria-label="关闭"
          >
            <X className="size-4" />
          </Button>
        </div>

        <form
          className="p-6"
          onSubmit={(e) => {
            e.preventDefault();
            login.mutate();
          }}
        >
          <label className="mb-2 block text-[11px] font-semibold text-black/55">
            登录产品
          </label>
          <div className="mb-5 grid grid-cols-3 gap-1 rounded-lg bg-black/[.045] p-1">
            {(
              [
                ["pedia", "斑马百科"],
                ["aioral", "斑马口语"],
                ["zebra", "斑马AI学"],
              ] as const
            ).map(([value, label]) => (
              <button
                type="button"
                key={value}
                onClick={() => setProduct(value)}
                className={cn(
                  "h-9 rounded-md text-xs font-medium transition",
                  product === value
                    ? "bg-white text-black shadow-sm"
                    : "text-black/45 hover:text-black/70",
                )}
              >
                {label}
              </button>
            ))}
          </div>

          <label
            htmlFor="phone"
            className="mb-2 block text-[11px] font-semibold text-black/55"
          >
            手机号
          </label>
          <div className="mb-4 flex h-11 items-center rounded-lg border border-black/12 bg-white focus-within:border-black/35">
            <span className="border-r border-black/8 px-3 text-xs text-black/45">
              +86
            </span>
            <input
              id="phone"
              autoFocus
              inputMode="tel"
              autoComplete="tel"
              maxLength={11}
              value={phone}
              onChange={(e) => setPhone(e.target.value.replace(/\D/g, ""))}
              className="min-w-0 flex-1 bg-transparent px-3 text-sm outline-none"
              placeholder="请输入手机号"
            />
          </div>

          <label
            htmlFor="sms-code"
            className="mb-2 block text-[11px] font-semibold text-black/55"
          >
            短信验证码
          </label>
          <div className="flex h-11 items-center rounded-lg border border-black/12 bg-white focus-within:border-black/35">
            <MessageSquareText className="ml-3 size-4 text-black/30" />
            <input
              id="sms-code"
              inputMode="numeric"
              autoComplete="one-time-code"
              maxLength={8}
              value={code}
              onChange={(e) => setCode(e.target.value.replace(/\D/g, ""))}
              className="min-w-0 flex-1 bg-transparent px-3 text-sm tracking-[.18em] outline-none"
              placeholder="验证码"
            />
            <button
              type="button"
              disabled={sms.isPending || countdown > 0 || phone.length !== 11}
              onClick={() => sms.mutate()}
              className="mr-2 min-w-24 rounded-md px-2 py-2 text-xs font-medium text-[#e54e3b] disabled:text-black/25"
            >
              {sms.isPending
                ? "发送中…"
                : countdown > 0
                  ? `${countdown} 秒后重发`
                  : "获取验证码"}
            </button>
          </div>

          {error && (
            <div className="mt-3 rounded-md bg-red-50 px-3 py-2 text-xs leading-5 text-red-700">
              {error}
            </div>
          )}
          <Button
            type="submit"
            variant="accent"
            className="mt-5 h-11 w-full"
            disabled={login.isPending || phone.length !== 11 || code.length < 4}
          >
            {login.isPending ? (
              <>
                <LoaderCircle className="size-4 animate-spin" />
                正在登录
              </>
            ) : (
              <>
                <KeyRound className="size-4" />
                登录并继续
              </>
            )}
          </Button>
          <p className="mt-4 flex items-start gap-2 text-[10px] leading-4 text-black/38">
            <ShieldCheck className="mt-0.5 size-3.5 shrink-0" />
            手机号和验证码仅用于登录请求；会话凭据保存在本机应用数据目录中，可通过“退出登录”清除。
          </p>
        </form>
      </section>
    </div>
  );
}

function readError(error: unknown) {
  if (error instanceof z.ZodError)
    return error.issues[0]?.message ?? "输入有误";
  return error instanceof Error ? error.message : String(error);
}
