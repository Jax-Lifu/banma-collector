import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  loginSessionSchema,
  productContentSchema,
  resourceListSchema,
  type ContentEntry,
  type ResourceItem,
  type ZebraProduct,
} from "./schema";

declare global {
  interface Window {
    __BANMA_E2E__?: {
      invoke: (command: string, args?: unknown) => Promise<unknown>;
    };
  }
}

function invokeCommand<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  const e2e =
    (import.meta as ImportMeta & { env?: Record<string, string | undefined> })
      .env?.VITE_E2E === "1";
  if (e2e && window.__BANMA_E2E__) {
    return window.__BANMA_E2E__.invoke(command, args) as Promise<T>;
  }
  return invoke<T>(command, args);
}

export async function chooseDirectory() {
  return open({ directory: true, multiple: false });
}
export async function downloadResources(
  items: ResourceItem[],
  outputDir: string,
  concurrency: number,
  product: ZebraProduct,
  separateLanguages: boolean,
) {
  return invokeCommand("download_resources", {
    request: { items, outputDir, concurrency, product, separateLanguages },
  });
}
export async function cancelDownload(id?: string) {
  return invokeCommand("cancel_download", { request: id ? { id } : null });
}
export async function revealPath(path: string) {
  return invokeCommand("reveal_path", { path });
}
export async function getResourcePreviewPath(
  item: ResourceItem,
  outputDir: string,
  separateLanguages: boolean,
) {
  return invokeCommand<string | null>("resource_preview_path", {
    request: { item, outputDir, separateLanguages },
  });
}
export async function requestSms(phone: string, product: ZebraProduct) {
  return invokeCommand("request_sms", { request: { phone, product } });
}
export async function phoneLogin(
  phone: string,
  code: string,
  product: ZebraProduct,
) {
  return loginSessionSchema.parse(
    await invokeCommand("phone_login", { request: { phone, code, product } }),
  );
}
export async function getLoginSession(product: ZebraProduct) {
  return loginSessionSchema.parse(
    await invokeCommand("login_session", { request: { product } }),
  );
}
export async function logout(product: ZebraProduct) {
  return loginSessionSchema.parse(
    await invokeCommand("logout", { request: { product } }),
  );
}
export async function loadProductCatalog(product: ZebraProduct) {
  return productContentSchema.parse(
    await invokeCommand("load_product_catalog", { request: { product } }),
  );
}
export async function loadContentDetail(
  product: ZebraProduct,
  entry: ContentEntry,
) {
  return productContentSchema.parse(
    await invokeCommand("load_content_detail", {
      request: {
        product,
        entryId: entry.id,
        entryTitle: entry.title,
        entryKind: entry.kind,
        parentId: entry.parentId,
        actionUrl: entry.actionUrl,
      },
    }),
  );
}
export async function loadAlbumsResources(
  product: ZebraProduct,
  albums: ContentEntry[],
) {
  const albumIds = albums.map((album) => album.id);
  const albumTitles = Object.fromEntries(
    albums.map((album) => [album.id, album.title]),
  );
  const raw = await invokeCommand("load_albums_resources", {
    request: { product, albumIds, albumTitles },
  });
  return resourceListSchema.parse(raw);
}
