import { useRef } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ImagePlus, Loader2, Trash2 } from "lucide-react";
import { api, keys } from "../../lib/api";
import { useLogoUrl } from "../../lib/logo";
import { useToast } from "../../components/Toast";
import { Button, Card } from "../../components/ui";
import { SettingsSection } from "./SettingsSection";

/** Business logo printed on invoices: PNG, JPEG or SVG up to 1 MB. */
export function LogoSection() {
  const qc = useQueryClient();
  const toast = useToast();
  const input = useRef<HTMLInputElement>(null);
  const url = useLogoUrl();
  const done = (msg: string) => {
    void qc.invalidateQueries({ queryKey: keys.logo });
    void qc.invalidateQueries({ queryKey: ["audit"] });
    toast(msg);
  };
  const upload = useMutation({
    mutationFn: async (f: File) => api.setLogo(f.name, new Uint8Array(await f.arrayBuffer())),
    onSuccess: () => done("Logo updated"),
    onError: (e) => toast(e.message, "error"),
  });
  const remove = useMutation({
    mutationFn: api.removeLogo,
    onSuccess: () => done("Logo removed"),
    onError: (e) => toast(e.message, "error"),
  });

  return (
    <SettingsSection title="Logo" description="Printed at the top of your invoices, next to the business name.">
      <Card className="flex items-center gap-5 p-5">
        <div className="grid h-20 w-36 shrink-0 place-items-center overflow-hidden rounded-xl border border-dashed border-line-strong bg-white p-2">
          {url ? <img src={url} alt="Business logo" className="max-h-full max-w-full object-contain" /> : <ImagePlus className="text-subtle" size={22} aria-hidden />}
        </div>
        <div className="min-w-0 flex-1">
          <p className="text-[13px] text-muted">PNG, JPEG or SVG, up to 1 MB. A wide logo on a transparent background prints best.</p>
          <div className="mt-3 flex gap-2">
            <Button type="button" disabled={upload.isPending} onClick={() => input.current?.click()}>
              {upload.isPending ? <Loader2 size={15} className="animate-spin" /> : <ImagePlus size={15} />} {url ? "Replace" : "Upload logo"}
            </Button>
            {url && (
              <Button type="button" variant="ghost" disabled={remove.isPending} onClick={() => remove.mutate()}>
                <Trash2 size={15} /> Remove
              </Button>
            )}
          </div>
          <input
            ref={input}
            type="file"
            accept="image/png,image/jpeg,image/svg+xml"
            className="hidden"
            onChange={(e) => {
              const f = e.target.files?.[0];
              if (f) upload.mutate(f);
              e.target.value = "";
            }}
          />
        </div>
      </Card>
    </SettingsSection>
  );
}
