import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api, keys } from "./api";

/**
 * Object URL of the business logo (or null). Created and revoked inside one
 * effect, so a re-run (React StrictMode, a new logo) never leaves a revoked URL
 * on screen.
 */
export function useLogoUrl(): string | null {
  const { data } = useQuery({ queryKey: keys.logo, queryFn: api.getLogo, staleTime: Infinity });
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    if (!data) {
      setUrl(null);
      return;
    }
    const u = URL.createObjectURL(new Blob([new Uint8Array(data.bytes)], { type: data.contentType }));
    setUrl(u);
    return () => URL.revokeObjectURL(u);
  }, [data]);
  return url;
}
