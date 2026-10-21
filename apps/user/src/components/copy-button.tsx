import { useState } from 'react'
import { Check, Copy } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'

export function CopyButton({
  text,
  label = '复制',
}: {
  text: string
  label?: string
}) {
  const [done, setDone] = useState(false)

  return (
    <Button
      type="button"
      variant="outline"
      size="sm"
      onClick={async () => {
        try {
          await navigator.clipboard.writeText(text)
          setDone(true)
          toast.success('已复制')
          window.setTimeout(() => setDone(false), 1500)
        } catch (err: unknown) {
          toast.error(err instanceof Error ? err.message : '复制失败')
        }
      }}
    >
      {done ? <Check /> : <Copy />}
      {label}
    </Button>
  )
}
