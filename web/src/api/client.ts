import createClient from "openapi-fetch";
import type { components, paths } from "./schema";

export type User = components["schemas"]["User"];
export type AuthStatus = components["schemas"]["AuthStatus"];
export type Invite = components["schemas"]["Invite"];
export type CreatedInvite = components["schemas"]["CreatedInvite"];
export type ThemeSummary = components["schemas"]["ThemeSummary"];
type ErrorBody = components["schemas"]["ErrorBody"];

/** Typed client generated from the server's OpenAPI file (`npm run api:generate`). */
export const api = createClient<paths>({ baseUrl: "/api/v1", credentials: "same-origin" });

export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
  ) {
    super(message);
  }
}

/** Returns `data`, or throws an `ApiError` carrying the server's message. */
export async function unwrap<T>(
  call: Promise<{ data?: T; error?: unknown; response: Response }>,
): Promise<T> {
  const { data, error, response } = await call;
  if (!response.ok) {
    const body = error as Partial<ErrorBody> | undefined;
    throw new ApiError(
      response.status,
      body?.code ?? "unknown",
      body?.message ?? `The server answered ${response.status}.`,
    );
  }
  return data as T;
}
