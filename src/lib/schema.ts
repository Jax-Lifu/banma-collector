import { z } from "zod";

export const resourceSchema = z.object({
  id: z.string(),
  title: z.string(),
  url: z.url(),
  kind: z.enum(["video", "audio", "image", "data", "document", "other"]),
  extension: z.string(),
  size: z.number().nullish(),
  source: z.string(),
  subfolder: z.string().nullish(),
  sequence: z.number().int().positive().nullish(),
  quality: z.string().nullish(),
  language: z.string().nullish(),
});
export const resourceListSchema = z.array(resourceSchema);
export const loginSessionSchema = z.object({
  loggedIn: z.boolean(),
  phoneMasked: z.string().nullish(),
  product: z.enum(["pedia", "aioral", "zebra"]).nullish(),
  userId: z.string().nullish(),
  nickname: z.string().nullish(),
});
export type ResourceItem = z.infer<typeof resourceSchema>;
export type ResourceKind = ResourceItem["kind"];
export type LoginSession = z.infer<typeof loginSessionSchema>;
export type ZebraProduct = "pedia" | "aioral" | "zebra";
export const contentEntrySchema = z.object({
  id: z.string(),
  title: z.string(),
  subtitle: z.string().nullish(),
  coverUrl: z.string().nullish(),
  kind: z.enum([
    "pack",
    "course",
    "mission",
    "episode",
    "unit",
    "album",
    "song",
    "audio",
    "book",
  ]),
  locked: z.boolean(),
  actionUrl: z.string().nullish(),
  parentId: z.string().nullish(),
  hasDetail: z.boolean().nullish(),
});
export const productContentSchema = z.object({
  entries: z.array(contentEntrySchema),
  videos: resourceListSchema,
  cursor: z.string().nullish(),
  warning: z.string().nullish(),
});
export type ContentEntry = z.infer<typeof contentEntrySchema>;
export type ProductContent = z.infer<typeof productContentSchema>;
export type ProgressState = {
  status: "queued" | "downloading" | "completed" | "failed" | "cancelled";
  received: number;
  total?: number | null;
  error?: string | null;
};
