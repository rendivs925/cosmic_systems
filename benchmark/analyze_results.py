#!/usr/bin/env python3
"""
Cosmic Systems Performance Analysis Tool
Generates detailed performance breakdowns and visualizations from benchmark results
"""

import json
import os
import glob
import matplotlib.pyplot as plt
import numpy as np
from datetime import datetime
import argparse
import sys
from typing import Dict, List, Any, Optional

class PerformanceAnalyzer:
    def __init__(self, metrics_dir: str):
        self.metrics_dir = metrics_dir
        self.results = {}
        self.baseline_config = None

    def load_metrics(self) -> Dict[str, Dict]:
        """Load all metrics JSON files from the metrics directory"""
        metrics_files = glob.glob(os.path.join(self.metrics_dir, "*_metrics.json"))

        for metrics_file in metrics_files:
            config_name = os.path.basename(metrics_file).replace("_metrics.json", "")

            try:
                with open(metrics_file, 'r') as f:
                    data = json.load(f)
                    self.results[config_name] = data

                    # Identify baseline (sequential or first result)
                    if self.baseline_config is None or "sequential" in config_name:
                        self.baseline_config = config_name

            except (json.JSONDecodeError, FileNotFoundError) as e:
                print(f"Warning: Could not load {metrics_file}: {e}")
                continue

        return self.results

    def calculate_improvements(self) -> Dict[str, Dict]:
        """Calculate performance improvements relative to baseline"""
        if not self.baseline_config or self.baseline_config not in self.results:
            return {}

        baseline_fps = self.results[self.baseline_config]["performance"]["avg_fps"]
        improvements = {}

        for config_name, data in self.results.items():
            if config_name == self.baseline_config:
                improvements[config_name] = {
                    "fps_improvement_percent": 0,
                    "relative_performance": 1.0,
                    "absolute_fps": data["performance"]["avg_fps"]
                }
            else:
                current_fps = data["performance"]["avg_fps"]
                if baseline_fps > 0:
                    improvement_percent = ((current_fps - baseline_fps) / baseline_fps) * 100
                    relative_perf = current_fps / baseline_fps
                else:
                    improvement_percent = 0
                    relative_perf = 1.0

                improvements[config_name] = {
                    "fps_improvement_percent": improvement_percent,
                    "relative_performance": relative_perf,
                    "absolute_fps": current_fps
                }

        return improvements

    def generate_ascii_chart(self, improvements: Dict[str, Dict]) -> str:
        """Generate ASCII bar chart of performance improvements"""
        if not improvements:
            return "No improvement data available"

        chart_lines = []
        chart_lines.append("Performance Improvement vs Baseline")
        chart_lines.append("=" * 50)

        # Sort by improvement percentage
        sorted_configs = sorted(improvements.items(),
                              key=lambda x: x[1]["fps_improvement_percent"],
                              reverse=True)

        max_name_len = max(len(name) for name in improvements.keys())
        max_bar_width = 40

        for config_name, data in sorted_configs:
            improvement = data["fps_improvement_percent"]
            fps = data["absolute_fps"]

            # Create bar
            if improvement >= 0:
                bar_length = min(int(improvement / 2), max_bar_width)  # Scale down for readability
                bar = "█" * bar_length
                color_code = "🟢" if improvement > 10 else "🟡"
            else:
                bar_length = min(int(abs(improvement) / 2), max_bar_width)
                bar = "░" * bar_length
                color_code = "🔴"

            config_display = f"{config_name:<{max_name_len}}"
            chart_lines.append(f"{config_display} {color_code} {bar} {improvement:+.1f}% ({fps:.1f} FPS)")

        return "\n".join(chart_lines)

    def generate_detailed_report(self, improvements: Dict[str, Dict]) -> str:
        """Generate detailed markdown report"""
        report_lines = []
        report_lines.append("# Cosmic Systems Performance Analysis Report\n")

        # Get sample data for hardware info
        sample_data = None
        if self.results:
            sample_data = next(iter(self.results.values()))

        # Hardware summary
        if sample_data:
            hw = sample_data.get("hardware", {})
        else:
            hw = {}

            report_lines.append("## Hardware Configuration\n")
            report_lines.append(f"- **CPU Model:** {hw.get('cpu_model', 'Unknown')}")
            report_lines.append(f"- **CPU Cores:** {hw.get('cpu_cores', 'Unknown')}")
            report_lines.append(f"- **Memory:** {hw.get('memory_gb', 'Unknown')}GB")
            report_lines.append(f"- **AVX-512 Support:** {'Yes' if hw.get('avx512_support') else 'No'}")
            report_lines.append(f"- **AVX-2 Support:** {'Yes' if hw.get('avx2_support') else 'No'}")
            report_lines.append("")

        # Performance summary
        report_lines.append("## Performance Summary\n")
        report_lines.append("| Configuration | FPS | Improvement | Relative Perf |")
        report_lines.append("|---------------|-----|-------------|---------------|")

        sorted_configs = sorted(improvements.items(),
                              key=lambda x: x[1]["fps_improvement_percent"],
                              reverse=True)

        for config_name, data in sorted_configs:
            fps = data["absolute_fps"]
            improvement = data["fps_improvement_percent"]
            relative = data["relative_performance"]
            report_lines.append(f"| {config_name} | {fps:.1f} | {improvement:+.1f}% | {relative:.2f}x |")

        report_lines.append("")

        # Optimization breakdown
        report_lines.append("## Optimization Impact Analysis\n")

        optimization_descriptions = {
            "adaptive_kepler": {
                "name": "Adaptive Kepler Solver",
                "description": "Distance-based iteration reduction for orbital calculations",
                "expected_impact": "40-60% physics time reduction",
                "scaling": "High (better with more planets at varying distances)"
            },
            "simd_only": {
                "name": "SIMD Vectorization",
                "description": "AVX-512/AVX2/AVX2 accelerated mathematical operations",
                "expected_impact": "3-16x Kepler calculation speedup",
                "scaling": "Very High (linear with SIMD width)"
            },
            "parallel_only": {
                "name": "Parallel Processing",
                "description": "Multi-core Kepler equation processing with Rayon",
                "expected_impact": "Near-linear scaling with CPU cores",
                "scaling": "High (optimal on high-core CPUs)"
            },
            "rendering_throttling": {
                "name": "Rendering Throttling",
                "description": "Frame-rate limited material and visual updates",
                "expected_impact": "70-80% GPU material update reduction",
                "scaling": "Medium (more beneficial on lower-end GPUs)"
            },
            "assembly_optimizations": {
                "name": "Assembly Optimizations",
                "description": "Hand-optimized assembly for critical math functions",
                "expected_impact": "10-50% transcendental function improvement",
                "scaling": "Platform-specific"
            }
        }

        for config_name, opt_data in optimization_descriptions.items():
            if config_name in improvements:
                improvement = improvements[config_name]["fps_improvement_percent"]
                report_lines.append(f"### {opt_data['name']}")
                report_lines.append(f"**Actual Improvement:** {improvement:+.1f}%")
                report_lines.append(f"**Expected Impact:** {opt_data['expected_impact']}")
                report_lines.append(f"**Description:** {opt_data['description']}")
                report_lines.append(f"**Hardware Scaling:** {opt_data['scaling']}")
                report_lines.append("")

        # Recommendations
        report_lines.append("## Recommendations\n")

        # Analyze hardware and provide recommendations
        if sample_data:
            hw = sample_data.get("hardware", {})
            cpu_cores = hw.get("cpu_cores", 4)
            has_avx512 = hw.get("avx512_support", False)
            has_avx2 = hw.get("avx2_support", False)

            if has_avx512:
                report_lines.append("**AVX-512 Capable System:**")
                report_lines.append("- SIMD optimizations provide the highest performance gains")
                report_lines.append("- Consider enabling assembly-level optimizations for maximum performance")
                report_lines.append("- Parallel processing scales exceptionally well on this hardware")
            elif has_avx2 and cpu_cores > 8:
                report_lines.append("**High-Core AVX2 System:**")
                report_lines.append("- Combine SIMD and parallel processing for optimal performance")
                report_lines.append("- Adaptive Kepler solver provides good scaling with planet count")
                report_lines.append("- Consider the optimized build for production use")
            else:
                report_lines.append("**Standard Hardware:**")
                report_lines.append("- Adaptive Kepler solver provides the best balance of performance and compatibility")
                report_lines.append("- Rendering throttling optimizations help on integrated graphics")
                report_lines.append("- Parallel processing provides good scaling with available cores")

        return "\n".join(report_lines)

    def create_visualization(self, improvements: Dict[str, Dict], output_file: str = "performance_chart.png"):
        """Create matplotlib visualization of performance improvements"""
        try:
            if not improvements:
                print("No improvement data available for visualization")
                return

            # Prepare data
            configs = []
            improvements_pct = []
            fps_values = []

            sorted_items = sorted(improvements.items(),
                                key=lambda x: x[1]["fps_improvement_percent"],
                                reverse=True)

            for config_name, data in sorted_items:
                configs.append(config_name.replace("_", " ").title())
                improvements_pct.append(data["fps_improvement_percent"])
                fps_values.append(data["absolute_fps"])

            # Create figure with subplots
            fig, (ax1, ax2) = plt.subplots(2, 1, figsize=(12, 8))

            # Bar chart for improvements
            bars = ax1.bar(configs, improvements_pct,
                          color=['red' if x < 0 else 'green' for x in improvements_pct])
            ax1.set_ylabel('Performance Improvement (%)')
            ax1.set_title('Performance Improvement vs Baseline Configuration')
            ax1.grid(True, alpha=0.3)

            # Rotate x-axis labels
            plt.setp(ax1.xaxis.get_majorticklabels(), rotation=45, ha='right')

            # Add value labels on bars
            for bar, pct in zip(bars, improvements_pct):
                height = bar.get_height()
                ax1.text(bar.get_x() + bar.get_width()/2., height + (1 if height >= 0 else -3),
                        f'{pct:+.1f}%',
                        ha='center', va='bottom' if height >= 0 else 'top', fontweight='bold')

            # Line chart for absolute FPS
            ax2.plot(configs, fps_values, 'bo-', linewidth=2, markersize=8)
            ax2.set_ylabel('Frames Per Second (FPS)')
            ax2.set_title('Absolute Performance (FPS)')
            ax2.grid(True, alpha=0.3)

            # Add FPS value labels
            for i, fps in enumerate(fps_values):
                ax2.text(i, fps + max(fps_values) * 0.02, f'{fps:.1f}',
                        ha='center', va='bottom', fontweight='bold')

            plt.tight_layout()
            plt.savefig(output_file, dpi=150, bbox_inches='tight')
            print(f"Visualization saved to: {output_file}")

        except ImportError:
            print("Warning: matplotlib not available, skipping visualization")
        except Exception as e:
            print(f"Warning: Could not create visualization: {e}")

def main():
    parser = argparse.ArgumentParser(description="Cosmic Systems Performance Analysis Tool")
    parser.add_argument("--metrics-dir", default="./benchmark/metrics",
                       help="Directory containing metrics JSON files")
    parser.add_argument("--output-dir", default="./benchmark/reports",
                       help="Directory to save analysis reports")
    parser.add_argument("--chart-file", default="performance_analysis.png",
                       help="Filename for the performance chart")
    parser.add_argument("--latest", action="store_true",
                       help="Analyze the most recent benchmark run")

    args = parser.parse_args()

    # Find the most recent metrics directory if requested
    if args.latest:
        benchmark_dir = os.path.dirname(args.metrics_dir)
        metrics_dirs = [d for d in os.listdir(benchmark_dir)
                       if os.path.isdir(os.path.join(benchmark_dir, d)) and d.startswith("20")]
        if metrics_dirs:
            # Sort by timestamp (directories are named with timestamps)
            metrics_dirs.sort(reverse=True)
            args.metrics_dir = os.path.join(benchmark_dir, metrics_dirs[0])
            print(f"Analyzing latest results: {args.metrics_dir}")
        else:
            print("No benchmark results found")
            return

    # Create output directory
    os.makedirs(args.output_dir, exist_ok=True)

    # Initialize analyzer
    analyzer = PerformanceAnalyzer(args.metrics_dir)

    # Load and analyze metrics
    results = analyzer.load_metrics()

    if not results:
        print(f"No metrics found in {args.metrics_dir}")
        return

    print(f"Loaded {len(results)} benchmark configurations")

    # Calculate improvements
    improvements = analyzer.calculate_improvements()

    # Generate reports
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")

    # ASCII chart
    chart = analyzer.generate_ascii_chart(improvements)
    print("\n" + "="*60)
    print(chart)
    print("="*60 + "\n")

    # Detailed markdown report
    detailed_report = analyzer.generate_detailed_report(improvements)
    report_file = os.path.join(args.output_dir, f"detailed_analysis_{timestamp}.md")
    with open(report_file, 'w') as f:
        f.write(detailed_report)
    print(f"Detailed report saved to: {report_file}")

    # Visualization
    chart_path = os.path.join(args.output_dir, args.chart_file)
    analyzer.create_visualization(improvements, chart_path)

    print("\nAnalysis complete!")
    print(f"Results: {len(results)} configurations analyzed")
    print(f"Baseline: {analyzer.baseline_config}")
    print(f"Reports saved to: {args.output_dir}")

if __name__ == "__main__":
    main()