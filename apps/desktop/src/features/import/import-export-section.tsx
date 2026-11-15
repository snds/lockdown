import { useState } from "react"

import { FieldRow, FieldRowAction, FieldRowValue } from "@unicron/ui"

import { ExportDialog } from "@/features/export/export-dialog"
import { useRowSize } from "@/features/settings/setting-rows"

import { ImportWizard } from "./import-wizard"

interface ImportExportSectionProps {
  onChanged: () => void
  onNotice: (text: string) => void
  /** The import's last step, FIND DUPLICATES: opens Health, where the duplicates review is. */
  onFindDuplicates?: () => void
}

/**
 * Settings → Import and export (brief T7, doc 83): two rows, IMPORT and
 * EXPORT, each opening its Form. The web vault lists no such section: its
 * client refuses both (brief W1).
 */
export function ImportExportSection({ onChanged, onNotice, onFindDuplicates }: ImportExportSectionProps) {
  const size = useRowSize()
  const [open, setOpen] = useState<"import" | "export" | null>(null)
  return (
    <div className="flex flex-col">
      <FieldRow size={size} label="Import" action={<FieldRowAction onClick={() => setOpen("import")}>Import…</FieldRowAction>}>
        <FieldRowValue truncate="wrap">From another app or a backup</FieldRowValue>
      </FieldRow>
      <FieldRow size={size} label="Export" rule="both" action={<FieldRowAction onClick={() => setOpen("export")}>Export…</FieldRowAction>}>
        <FieldRowValue truncate="wrap">One file of your vault</FieldRowValue>
      </FieldRow>
      <ImportWizard
        open={open === "import"}
        onClose={() => setOpen(null)}
        onImported={(report) => {
          onNotice(`${report.imported} ${report.imported === 1 ? "item" : "items"} brought in`)
          onChanged()
        }}
        onFindDuplicates={onFindDuplicates}
      />
      <ExportDialog open={open === "export"} onClose={() => setOpen(null)} onNotice={onNotice} />
    </div>
  )
}
