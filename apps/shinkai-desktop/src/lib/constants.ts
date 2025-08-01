import { cn } from '@shinkai_network/shinkai-ui/utils';
import {
  type TreePassThroughMethodOptions,
  type TreePassThroughOptions,
} from 'primereact/tree';

export const allowedFileExtensions = [
  'csv',
  'docx',
  'html',
  'json',
  'md',
  'pdf',
  'txt',
  'xlsx',
];

export const treeOptions: TreePassThroughOptions = {
  root: {
    className: cn(
      '',
      '!bg-bg-default !border-divider my-0 w-full rounded-md border !px-1 !py-1.5 text-white',
    ),
  },
  label: { className: 'text-white text-sm line-clamp-1 break-all' },
  container: {
    className: 'm-0 !p-0 list-none overflow-auto',
  },
  node: { className: 'p-0 outline-hidden' },
  content: {
    className: cn(
      'text-official-gray-400 hover:bg-official-gray-850 mb-1 rounded-lg !bg-transparent p-1 !py-0 shadow-none hover:text-white',
      'cursor-pointer select-none',
    ),
  },
  toggler: ({ context }: TreePassThroughMethodOptions) => ({
    className: cn(
      'relative inline-flex shrink-0 cursor-pointer items-center justify-center overflow-hidden select-none',
      'mr-2.5 h-4 w-4 rounded-full border-0 !bg-transparent transition duration-200',
      context.selected
        ? 'hover:bg-official-gray-850 text-brand'
        : 'hover:text-gray-80 hover:bg-official-gray-850 text-white',
      // @ts-expect-error - TS doesn't know about the isLeaf property
      context.isLeaf && 'invisible',
    ),
  }),

  nodeIcon: { className: 'mr-2 ' },
  subgroup: {
    className: cn('m-0 list-none', 'space-y-1 p-0 pl-4'),
  },
};
