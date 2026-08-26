import { beforeEach, describe, expect, it } from "vitest";
import type { ResourceItem } from "../lib/schema";
import { useCollectorStore } from "./collector";

const resource = (id: string, source: string): ResourceItem => ({
  id,
  title: id,
  url: `https://example.test/${id}`,
  kind: "video",
  extension: "mp4",
  source,
});

describe("collector product state", () => {
  beforeEach(() => {
    useCollectorStore.setState({
      products: {
        pedia: { resources: [], selected: [], progress: {} },
        aioral: { resources: [], selected: [], progress: {} },
        zebra: { resources: [], selected: [], progress: {} },
      },
    });
  });

  it("隔离不同产品的资源、选择和下载进度", () => {
    const store = useCollectorStore.getState();
    store.setResources("pedia", [resource("shared-id", "pedia")]);
    store.updateProgress("pedia", "shared-id", {
      status: "downloading",
      received: 1024,
    });
    store.setResources("zebra", [resource("shared-id", "zebra")]);

    const products = useCollectorStore.getState().products;
    expect(products.pedia.progress["shared-id"]).toMatchObject({
      status: "downloading",
      received: 1024,
    });
    expect(products.zebra.progress["shared-id"]).toBeUndefined();
    expect(products.pedia.resources[0].source).toBe("pedia");
    expect(products.zebra.resources[0].source).toBe("zebra");
  });

  it("刷新同一产品的资源列表时保留后台下载进度", () => {
    const store = useCollectorStore.getState();
    store.updateProgress("pedia", "course-1", {
      status: "downloading",
      received: 2048,
    });
    store.setResources("pedia", [resource("course-1", "pedia")]);

    expect(
      useCollectorStore.getState().products.pedia.progress["course-1"],
    ).toMatchObject({ status: "downloading", received: 2048 });
  });

  it("批量提交一帧内的最新进度，同时保持产品隔离", () => {
    useCollectorStore.getState().updateProgressBatch([
      {
        product: "pedia",
        id: "course-1",
        value: { status: "downloading", received: 4096 },
      },
      {
        product: "zebra",
        id: "course-1",
        value: { status: "completed", received: 8192 },
      },
    ]);

    const products = useCollectorStore.getState().products;
    expect(products.pedia.progress["course-1"].received).toBe(4096);
    expect(products.zebra.progress["course-1"].received).toBe(8192);
  });
});
