import { expect, test } from "@playwright/test";

test("解析、选择、下载和停止形成完整闭环", async ({ page }) => {
  await page.addInitScript(() => {
    let finishDownload: (() => void) | undefined;
    window.__BANMA_E2E__ = {
      invoke: async (command: string) => {
        if (command === "login_session")
          return { loggedIn: true, maskedPhone: "152****8940" };
        if (command === "load_product_catalog")
          return {
            title: "斑马百科资源库",
            breadcrumb: [],
            entries: [
              {
                id: "moon",
                title: "月球",
                kind: "pack",
                locked: false,
                hasDetail: true,
              },
            ],
            videos: [],
          };
        if (command === "load_albums_resources")
          return [
            {
              id: "moon-1080",
              title: "认识月球",
              url: "https://example.test/moon.mpd",
              kind: "video",
              extension: "mpd",
              source: "pedia",
              subfolder: "月球/认识月球",
              sequence: 1,
              quality: "1080P",
              language: "中文",
            },
          ];
        if (command === "download_resources") {
          await new Promise<void>((resolve) => {
            finishDownload = resolve;
          });
          return null;
        }
        if (command === "cancel_download") {
          finishDownload?.();
          return null;
        }
        if (command === "resource_preview_path") return null;
        return null;
      },
    };
  });

  await page.goto("/pedia");
  await expect(page.getByText("月球", { exact: true })).toBeVisible();
  await page.getByTitle("选择此专辑批量下载").click();
  await page.getByRole("button", { name: /仅解析所选专辑/ }).click();
  await expect(page.getByText("认识月球", { exact: true })).toBeVisible();
  await expect(page.getByText(/尚未开始下载/)).toBeVisible();
  await page.getByRole("button", { name: /下载所选内容/ }).click();
  await expect(
    page.getByRole("button", { name: "停止全部下载" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "停止全部下载" }).click();
  await expect(page.getByText("下载任务已停止")).toBeVisible();
});
