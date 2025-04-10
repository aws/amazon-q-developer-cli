import {
  useRef,
  MutableRefObject,
  useImperativeHandle,
  ForwardRefRenderFunction,
  forwardRef,
} from "react";
import { FixedSizeList as List } from "react-window";
import { twMerge } from "tailwind-merge";
import { useDynamicResizeObserver } from "../hooks/helpers";

type ResizeHandler = (size: { width?: number; height?: number }) => void;

type AutoSizedListProps = Omit<List["props"], "height" | "width"> & {
  width: number | string | undefined;
  onResize: ResizeHandler | undefined;
};

export type AutoSizedHandleRef = {
  scrollToItem: (index: number) => void;
};

// List will attempt to be size (itemCount * itemSize) but will shrink and
// scroll if necessary.
const AutoSizedList: ForwardRefRenderFunction<
  AutoSizedHandleRef,
  AutoSizedListProps
> = (
  {
    onResize = undefined,
    width: desiredWidth = undefined,
    className,
    ...props
  }: AutoSizedListProps,
  ref,
) => {
  const { itemCount, itemSize } = props;
  const {
    ref: wrapperRef,
    height,
    width,
  } = useDynamicResizeObserver({ onResize });

  // Scroll when selectedIndex changes.
  const listRef = useRef<List>() as MutableRefObject<List>;
  useImperativeHandle(ref, () => ({
    scrollToItem: (index) => listRef.current.scrollToItem(index, "smart"),
  }));

  return (
    <div
      ref={wrapperRef}
      style={{
        flexBasis: itemCount * itemSize,
      }}
      className="min-h-0 min-w-0 flex-shrink"
    >
      <List
        width={desiredWidth === undefined ? width || 0 : desiredWidth}
        ref={listRef}
        height={height || 0}
        className={twMerge("scrollbar-none", className)}
        {...props}
      />
    </div>
  );
};

const AutoSizedListWrapped = forwardRef(AutoSizedList);

export default AutoSizedListWrapped;
