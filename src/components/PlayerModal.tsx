import { convertFileSrc } from "@tauri-apps/api/core";
import { Modal } from "./ui";

export function PlayerModal({
  open,
  src,
  title,
  subtitle,
  onClose,
  muted,
  autoPlay,
}: {
  open: boolean;
  src: string | null;
  title: string;
  subtitle?: string;
  onClose: () => void;
  muted?: boolean;
  autoPlay?: boolean;
}) {
  return (
    <Modal open={open} title={title} subtitle={subtitle} onClose={onClose} wide>
      {src ? (
        <div className="player-frame">
          <video
            key={src}
            src={convertFileSrc(src)}
            controls
            autoPlay={autoPlay ?? true}
            playsInline
            muted={muted}
          />
        </div>
      ) : (
        <p className="muted">El archivo no está disponible.</p>
      )}
    </Modal>
  );
}
