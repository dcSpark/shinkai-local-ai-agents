import { useMediaQuery } from '@uidotdev/usehooks';

export function useIsMobile() {
  return useMediaQuery('(max-width: 768px)');
}
