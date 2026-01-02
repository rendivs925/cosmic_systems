#!/bin/bash

# Performance Benchmark Script for Cosmic Systems Simulation
# Demonstrates the benefits of SIMD, parallel processing, and optimization features

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

echo "Cosmic Systems Simulation - Performance Benchmark"
echo "================================================="
echo ""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Check system requirements
check_requirements() {
    echo "Checking system requirements..."

    # Check if make is available
    if ! command -v make &> /dev/null; then
        echo -e "${RED}Error: make is required but not installed.${NC}"
        exit 1
    fi

    # Check if cargo is available
    if ! command -v cargo &> /dev/null; then
        echo -e "${RED}Error: cargo is required but not installed.${NC}"
        exit 1
    fi

    # Check CPU cores
    CPU_CORES=$(nproc)
    echo "Detected $CPU_CORES CPU cores"

    # Check SIMD support
    if lscpu | grep -q avx512; then
        SIMD_SUPPORT="AVX-512"
    elif lscpu | grep -q avx2; then
        SIMD_SUPPORT="AVX-2"
    else
        SIMD_SUPPORT="Basic"
    fi
    echo "SIMD Support: $SIMD_SUPPORT"
    echo ""
}

# Build all configurations
build_all() {
    echo -e "${BLUE}Building all configurations...${NC}"
    echo ""

    echo -e "${YELLOW}Building sequential (baseline)...${NC}"
    make build-release > /dev/null 2>&1 || {
        echo -e "${RED}Failed to build sequential version${NC}"
        return 1
    }

    echo -e "${YELLOW}Building parallel processing...${NC}"
    make build-parallel > /dev/null 2>&1 || {
        echo -e "${RED}Failed to build parallel version${NC}"
        return 1
    }

    echo -e "${YELLOW}Building SIMD + parallel...${NC}"
    make build-simd > /dev/null 2>&1 || {
        echo -e "${RED}Failed to build SIMD version${NC}"
        return 1
    }

    echo -e "${YELLOW}Building maximum optimization...${NC}"
    make build-optimized > /dev/null 2>&1 || {
        echo -e "${RED}Failed to build optimized version${NC}"
        return 1
    }

    echo -e "${GREEN}All builds completed successfully!${NC}"
    echo ""
}

# Run performance test for a configuration
run_perf_test() {
    local name="$1"
    local command="$2"
    local timeout_seconds=45

    echo -e "${CYAN}Testing $name...${NC}"

    # Run the test and capture timing
    local start_time=$(date +%s.%3N)
    timeout $timeout_seconds bash -c "$command" > /dev/null 2>&1 &
    local pid=$!
    wait $pid 2>/dev/null
    local exit_code=$?
    local end_time=$(date +%s.%3N)

    if [ $exit_code -eq 124 ]; then
        # Timeout occurred - normal for sustained testing
        local duration=$(echo "$end_time - $start_time" | bc)
        echo -e "${GREEN}  Completed $timeout_seconds seconds${NC}"
        printf "%.2f\n" $duration
    elif [ $exit_code -eq 0 ]; then
        # Completed normally
        local duration=$(echo "$end_time - $start_time" | bc)
        echo -e "${GREEN}  Completed successfully${NC}"
        printf "%.2f\n" $duration
    else
        echo -e "${RED}  Failed with exit code $exit_code${NC}"
        echo "0.00"
    fi
}

# Run comprehensive performance tests
run_performance_tests() {
    echo -e "${BLUE}Running comprehensive performance tests...${NC}"
    echo "Each test runs for 45 seconds of sustained simulation"
    echo ""

    local results_file=$(mktemp)
    echo "Configuration,Duration(s),FPS,Notes" > "$results_file"

    # Test sequential
    echo -e "${YELLOW}Sequential (baseline)${NC}"
    local seq_time=$(run_perf_test "Sequential" "cd $PROJECT_DIR && cargo run --release")
    echo "sequential,$seq_time,N/A,Baseline performance" >> "$results_file"

    # Test parallel
    echo -e "${YELLOW}Parallel processing${NC}"
    local par_time=$(run_perf_test "Parallel" "cd $PROJECT_DIR && make run-parallel")
    echo "parallel,$par_time,N/A,Rayon multi-core processing" >> "$results_file"

    # Test SIMD + parallel
    echo -e "${YELLOW}SIMD + parallel${NC}"
    local simd_time=$(run_perf_test "SIMD+Parallel" "cd $PROJECT_DIR && make run-simd")
    echo "simd_parallel,$simd_time,N/A,Vectorized math + multi-core" >> "$results_file"

    # Test optimized
    echo -e "${YELLOW}Maximum optimization${NC}"
    local opt_time=$(run_perf_test "Optimized" "cd $PROJECT_DIR && make run-optimized")
    echo "optimized,$opt_time,N/A,Maximum compiler optimizations" >> "$results_file"

    echo ""
    echo -e "${GREEN}Performance Test Results:${NC}"
    echo "=========================="
    column -t -s, "$results_file"

    echo ""
    echo -e "${CYAN}Analysis:${NC}"
    echo "----------"

    # Calculate improvements
    if (( $(echo "$par_time < $seq_time" | bc -l) )); then
        local par_improvement=$(echo "scale=1; ($seq_time - $par_time) / $seq_time * 100" | bc)
        echo -e "${GREEN}Parallel processing shows ${par_improvement}% improvement over sequential${NC}"
    else
        echo -e "${YELLOW}Parallel processing overhead may exceed benefits for this workload${NC}"
    fi

    if (( $(echo "$opt_time < $seq_time" | bc -l) )); then
        local opt_improvement=$(echo "scale=1; ($seq_time - $opt_time) / $seq_time * 100" | bc)
        echo -e "${GREEN}Maximum optimization shows ${opt_improvement}% improvement over sequential${NC}"
    fi

    echo ""
    echo -e "${BLUE}Key Findings:${NC}"
    echo "- Parallel processing benefits scale with CPU core count"
    echo "- SIMD optimizations help with vectorizable math operations"
    echo "- Compiler optimizations provide consistent performance gains"
    echo "- Performance benefits are most visible in sustained workloads"

    # Cleanup
    rm -f "$results_file"
}

# Memory usage analysis
run_memory_analysis() {
    echo -e "${BLUE}Memory Usage Analysis...${NC}"
    echo ""

    # Test sequential memory
    echo -e "${YELLOW}Sequential memory usage:${NC}"
    timeout 20s make run-release > /dev/null 2>&1 &
    sleep 3
    ps aux --no-headers -o pid,ppid,cmd,%mem,%cpu --sort=-%mem | head -3 | grep cosmic_systems || echo "Process not found"
    killall cosmic_systems 2>/dev/null || true

    # Test parallel memory
    echo -e "${YELLOW}Parallel memory usage:${NC}"
    timeout 20s make run-parallel > /dev/null 2>&1 &
    sleep 3
    ps aux --no-headers -o pid,ppid,cmd,%mem,%cpu --sort=-%mem | head -3 | grep cosmic_systems || echo "Process not found"
    killall cosmic_systems 2>/dev/null || true

    echo ""
}

# Main execution
main() {
    check_requirements

    echo "Available test modes:"
    echo "1. build     - Build all configurations"
    echo "2. perf      - Run performance tests only"
    echo "3. memory    - Run memory analysis only"
    echo "4. full      - Run complete benchmark suite"
    echo ""

    case "${1:-full}" in
        "build")
            build_all
            ;;
        "perf")
            build_all
            run_performance_tests
            ;;
        "memory")
            build_all
            run_memory_analysis
            ;;
        "full")
            build_all
            run_performance_tests
            run_memory_analysis
            ;;
        *)
            echo -e "${RED}Invalid mode. Use: build, perf, memory, or full${NC}"
            exit 1
            ;;
    esac

    echo ""
    echo -e "${GREEN}Benchmark completed!${NC}"
    echo ""
    echo "For more detailed profiling, run:"
    echo "  make flamegraph     - Generate flame graph"
    echo "  make cpu-profile    - CPU usage profiling"
    echo "  make memory-profile - Memory profiling"
}

# Run main function with provided arguments
main "$@"