import { useState } from "react";
import { useMutation, useQueryClient, type QueryKey } from "@tanstack/react-query";
import { ApiError } from "./api";
import { useToast } from "../components/Toast";

/**
 * Wraps a save action with the app's conventions: field errors land inline,
 * conflicts and other failures surface as a toast, success invalidates the
 * given queries (plus the audit log and dashboard) and confirms with a toast.
 */
export function useSave<TInput, TResult>(
  fn: (input: TInput) => Promise<TResult>,
  opts: { invalidate: QueryKey[]; success: string; onSuccess?: (r: TResult) => void },
) {
  const qc = useQueryClient();
  const toast = useToast();
  const [fieldErrors, setFieldErrors] = useState<Record<string, string>>({});

  const mutation = useMutation({
    mutationFn: fn,
    onMutate: () => setFieldErrors({}),
    onSuccess: (r) => {
      for (const k of [...opts.invalidate, ["audit"], ["dashboard"], ["report"]]) void qc.invalidateQueries({ queryKey: k });
      if (opts.success) toast(opts.success);
      opts.onSuccess?.(r);
    },
    onError: (e) => {
      if (e instanceof ApiError && Object.keys(e.fields).length) {
        setFieldErrors(e.fields);
        return;
      }
      if (e instanceof ApiError && e.code === "conflict") {
        for (const k of opts.invalidate) void qc.invalidateQueries({ queryKey: k });
      }
      toast(e.message, "error");
    },
  });

  return {
    ...mutation,
    fieldErrors,
    clearError: (field: string) =>
      setFieldErrors((f) => {
        if (!(field in f)) return f;
        const next = { ...f };
        delete next[field];
        return next;
      }),
    reset: () => {
      setFieldErrors({});
      mutation.reset();
    },
  };
}
