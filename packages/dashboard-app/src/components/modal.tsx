export default function Modal({ children }: { children: React.ReactNode }) {
  return (
    <div className="fixed z-50 inset-0 h-full w-full bg-white/70 dark:bg-black/50 backdrop-blur-lg flex justify-center items-center overflow-auto">
      <div className="p-10 rounded-lg bg-white dark:bg-zinc-800 flex flex-col shadow-2xl w-[400px]">
        {children}
      </div>
    </div>
  );
}
