import type { HTMLAttributes, Ref } from "react";
import { cn } from "@/lib/utils";

function Card({ className, style, ref, ...props }: HTMLAttributes<HTMLDivElement> & { ref?: Ref<HTMLDivElement> }) {
  return (
    <div
      ref={ref}
      className={cn("rounded-[8px] border", className)}
      style={{
        background: "var(--aurora-panel-medium)",
        borderColor: "var(--aurora-border-default)",
        boxShadow: "var(--aurora-shadow-medium), inset 0 1px 0 rgba(255,255,255,0.04)",
        ...style,
      }}
      {...props}
    />
  );
}
Card.displayName = "Card";

function CardHeader({ className, style, ref, ...props }: HTMLAttributes<HTMLDivElement> & { ref?: Ref<HTMLDivElement> }) {
  return (
    <div
      ref={ref}
      className={cn("border-b px-4 py-3", className)}
      style={{ borderColor: "var(--aurora-border-default)", ...style }}
      {...props}
    />
  );
}
CardHeader.displayName = "CardHeader";

function CardTitle({ className, style, ref, ...props }: HTMLAttributes<HTMLHeadingElement> & { ref?: Ref<HTMLHeadingElement> }) {
  return (
    <h3
      ref={ref}
      className={cn("aurora-text-section", className)}
      style={{ color: "var(--aurora-text-primary)", fontSize: 17, ...style }}
      {...props}
    />
  );
}
CardTitle.displayName = "CardTitle";

function CardDescription({ className, style, ref, ...props }: HTMLAttributes<HTMLParagraphElement> & { ref?: Ref<HTMLParagraphElement> }) {
  return (
    <p
      ref={ref}
      className={cn("aurora-text-body-sm", className)}
      style={{ color: "var(--aurora-text-muted)", marginTop: 4, ...style }}
      {...props}
    />
  );
}
CardDescription.displayName = "CardDescription";

function CardContent({ className, ref, ...props }: HTMLAttributes<HTMLDivElement> & { ref?: Ref<HTMLDivElement> }) {
  return (
    <div ref={ref} className={cn("px-4 py-3", className)} {...props} />
  );
}
CardContent.displayName = "CardContent";

function CardFooter({ className, style, ref, ...props }: HTMLAttributes<HTMLDivElement> & { ref?: Ref<HTMLDivElement> }) {
  return (
    <div
      ref={ref}
      className={cn("border-t px-4 py-3", className)}
      style={{ borderColor: "var(--aurora-border-default)", ...style }}
      {...props}
    />
  );
}
CardFooter.displayName = "CardFooter";

export { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle };
export default Card;
