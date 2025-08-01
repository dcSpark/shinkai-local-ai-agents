import { Toggle } from '@shinkai_network/shinkai-ui';
import { SortingAToZ, SortingZToA } from '@shinkai_network/shinkai-ui/assets';

import { useVectorFsStore } from '../context/vector-fs-context';

export default function VectorFsToggleSortName() {
  const isSortByName = useVectorFsStore((state) => state.isSortByName);
  const setSortByName = useVectorFsStore((state) => state.setSortByName);

  return (
    <Toggle
      aria-label="Toggle sort by name"
      className="bg-bg-tertiary text-text-default data-[state=on]:bg-bg-quaternary data-[state=on]:text-text-default"
      onPressedChange={() => {
        setSortByName(!isSortByName);
      }}
      pressed={isSortByName}
    >
      {isSortByName ? (
        <SortingAToZ className="h-[18px] w-[18px]" />
      ) : (
        <SortingZToA className="h-[18px] w-[18px]" />
      )}
    </Toggle>
  );
}
