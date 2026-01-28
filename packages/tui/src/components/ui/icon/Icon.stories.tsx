import { Icon, IconType, type IconProps } from './Icon.js';

export default {
  component: Icon,
};

export const Dot = {
  args: {
    type: IconType.DOT,
  } as IconProps,
};

export const SmallDot = {
  args: {
    type: IconType.SMALL_DOT,
  } as IconProps,
};

export const ChevronRight = {
  args: {
    type: IconType.CHEVRON_RIGHT,
  } as IconProps,
};
