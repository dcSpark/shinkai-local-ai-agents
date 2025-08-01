import { DownloadIcon } from '@radix-ui/react-icons';
import { Check } from 'lucide-react';
import React, { useState } from 'react';
import { type IProps, QRCode as ReactQRCode } from 'react-qrcode-logo';

import shinkaiLogo from '../assets/images/app-icon.png';
import { Button } from './button';
import { Dialog, DialogContent } from './dialog';

export function QRCode({
  value,
  size,
  id,
  ...props
}: {
  value: IProps['value'];
  size: IProps['size'];
  id?: IProps['id'];
} & React.HTMLAttributes<HTMLDivElement>): React.ReactElement {
  return (
    <div {...props}>
      <ReactQRCode
        ecLevel={'M'}
        eyeColor="black"
        eyeRadius={10}
        fgColor="black"
        id={id}
        logoHeight={size ? size * 0.2 : undefined}
        logoImage={shinkaiLogo}
        logoPaddingStyle="circle"
        logoWidth={size ? size * 0.2 : undefined}
        qrStyle="dots"
        removeQrCodeBehindLogo
        size={size ?? 300}
        value={value}
      />
    </div>
  );
}

export function QrCodeModal({
  open,
  onOpenChange,
  value,
  onSave,
  title,
  description,
  modalClassName,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  value: IProps['value'];
  onSave: (key: string) => void;
  title: React.ReactNode;
  description: React.ReactNode;
  modalClassName?: string;
}) {
  const [saved, setSaved] = useState(false);
  const downloadCode = async () => {
    const canvas = document.querySelector('#registration-code-qr');
    if (canvas instanceof HTMLCanvasElement) {
      try {
        const pngUrl = canvas.toDataURL();
        onSave(pngUrl);
        setSaved(true);
      } catch (error) {
        console.error(error);
      } finally {
        setTimeout(() => {
          setSaved(false);
        }, 3000);
      }
    }
  };

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent className={modalClassName}>
        <div className="flex flex-col items-center py-4">
          <h2 className="mb-1 text-lg font-semibold">{title}</h2>
          <p className="text-text-secondary mb-5 text-center text-xs">
            {description}
          </p>
          <div className="mb-7 overflow-hidden rounded-lg shadow-2xl">
            <QRCode size={190} value={value} />
            <QRCode
              className="hidden"
              id="registration-code-qr"
              size={1024}
              value={value}
            />
          </div>
          <div className="flex flex-col gap-4">
            <Button className="flex gap-1" onClick={downloadCode}>
              {saved ? <Check /> : <DownloadIcon className="h-4 w-4" />}
              {saved ? 'Saved' : 'Download'}
            </Button>
            <Button
              className="flex gap-1"
              onClick={() => {
                onOpenChange(false);
              }}
              variant="ghost"
            >
              I saved it, close
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
