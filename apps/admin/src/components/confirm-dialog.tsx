import { useState } from 'react'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'

export function ConfirmDialog({
  open,
  title,
  description,
  confirmLabel = '确认',
  destructive = false,
  busy = false,
  onConfirm,
  onOpenChange,
}: {
  open: boolean
  title: string
  description: string
  confirmLabel?: string
  destructive?: boolean
  busy?: boolean
  onConfirm: () => void
  onOpenChange: (open: boolean) => void
}) {
  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{title}</AlertDialogTitle>
          <AlertDialogDescription>{description}</AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={busy}>取消</AlertDialogCancel>
          <AlertDialogAction
            variant={destructive ? 'destructive' : 'default'}
            disabled={busy}
            onClick={(event) => {
              event.preventDefault()
              onConfirm()
            }}
          >
            {confirmLabel}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}

type ConfirmRequest = {
  title: string
  description: string
  confirmLabel?: string
  destructive?: boolean
  run: () => Promise<void> | void
}

export function useConfirm() {
  const [pending, setPending] = useState<ConfirmRequest | null>(null)
  const [busy, setBusy] = useState(false)

  function ask(request: ConfirmRequest) {
    setPending(request)
  }

  async function run() {
    if (!pending) return
    setBusy(true)
    try {
      await pending.run()
      setPending(null)
    } finally {
      setBusy(false)
    }
  }

  const dialog = (
    <ConfirmDialog
      open={pending !== null}
      title={pending?.title ?? ''}
      description={pending?.description ?? ''}
      confirmLabel={pending?.confirmLabel}
      destructive={pending?.destructive}
      busy={busy}
      onConfirm={() => void run()}
      onOpenChange={(open) => {
        if (!open && !busy) setPending(null)
      }}
    />
  )

  return { ask, dialog }
}
