import { Button } from '@shinkai_network/shinkai-ui';
import { ArrowLeft } from 'lucide-react';
import React from 'react';
import { useNavigate } from 'react-router';

interface MobileBackNavigationProps {
  title: string;
  onBack?: () => void;
}

const MobileBackNavigation: React.FC<MobileBackNavigationProps> = ({
  title,
  onBack,
}) => {
  const navigate = useNavigate();

  const handleBack = () => {
    if (onBack) {
      onBack();
    } else {
      void navigate('/settings');
    }
  };

  return (
    <div className="border-divider bg-bg-secondary border-b px-4 py-3 md:hidden">
      <div className="flex items-center gap-3">
        <Button
          variant="tertiary"
          size="icon"
          onClick={handleBack}
          className="hover:bg-bg-dark p-2 text-white"
        >
          <ArrowLeft className="h-5 w-5" />
          <span className="sr-only">Go back</span>
        </Button>
        <h1 className="text-lg font-semibold text-white">{title}</h1>
      </div>
    </div>
  );
};

export default MobileBackNavigation;
